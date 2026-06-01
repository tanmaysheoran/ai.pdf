//! Page text extraction with an OCR fallback for scanned pages.
//!
//! Text extraction uses `lopdf`. OCR shells out to the `tesseract` CLI when
//! available; there is no good pure-Rust OCR, so when `tesseract` is missing we
//! degrade gracefully (auto mode keeps whatever text we have; force mode errors
//! with an install hint).

use super::{IngestOptions, OcrMode};
use crate::source::xml_escape;
use crate::{AipdfError, Result};
use lopdf::{Document, Object};
use std::process::Command;

// ── Form XObject text extraction ──────────────────────────────────────────────
//
// lopdf::Document::extract_text processes only a page's direct content stream.
// Designer-tool PDFs (Figma, InDesign, Illustrator) place significant text
// inside Form XObjects — reusable content groups embedded as /XObject /Subtype
// /Form on each page — which lopdf silently skips.  The functions below decode
// those streams directly so the missed paragraphs are recovered.

/// Below this many extractable chars, a page is treated as "scanned" and is a
/// candidate for OCR (in auto mode).
const TEXT_THRESHOLD: usize = 16;

pub(super) fn extract_semantic_xml(doc: &Document, opts: &IngestOptions) -> Result<String> {
    let pages = doc.get_pages();
    let mut blocks: Vec<String> = Vec::new();
    let mut bid = 1usize;

    for (pageno, page_id) in pages {
        let embedded = if opts.ocr == OcrMode::Force {
            String::new()
        } else {
            let mut t = doc.extract_text(&[pageno]).unwrap_or_default();
            // Supplement with text from Form XObjects, which extract_text skips.
            let form_text = form_xobject_text(doc, page_id);
            if !form_text.trim().is_empty() {
                t.push('\n');
                t.push_str(&form_text);
            }
            t
        };

        let text = if matches!(opts.ocr, OcrMode::Auto | OcrMode::Force)
            && embedded.trim().len() < TEXT_THRESHOLD
        {
            match ocr_page(doc, pageno, &opts.lang) {
                Some(t) if !t.trim().is_empty() => t,
                _ => embedded,
            }
        } else {
            embedded
        };

        blocks.push(format!(
            r#"<section id="s{pageno}" level="1" page="{pageno}">"#
        ));
        let mut emitted = false;
        for para in split_paragraphs(&text) {
            let para = strip_garbled_tokens(&para);
            if para.is_empty() {
                continue;
            }
            blocks.push(format!(
                r#"<paragraph id="b{bid}" page="{pageno}" role="paragraph">{}</paragraph>"#,
                xml_escape(&para)
            ));
            bid += 1;
            emitted = true;
        }
        if !emitted {
            // Sections must be non-empty per the schema.
            blocks.push(format!(
                r#"<paragraph id="b{bid}" page="{pageno}" role="paragraph">(no extractable text on this page)</paragraph>"#
            ));
            bid += 1;
        }
        blocks.push("</section>".to_string());
    }

    if blocks.is_empty() {
        return Err(AipdfError::Pdf("PDF has no pages".into()));
    }

    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<document version=\"1.0\" id=\"ingested\" lang=\"en\">\n{}\n</document>",
        blocks.join("\n")
    ))
}

/// Returns true when a single whitespace-delimited token looks like
/// font-encoding garbage. PDFs that use a custom display font (Symbol,
/// Dingbats, or a bespoke glyph set) without a ToUnicode map cause lopdf to
/// map raw glyph IDs to Latin-1, producing tokens full of non-printable or
/// high-codepoint characters.
fn looks_garbled(text: &str) -> bool {
    let text = text.trim();
    let total = text.chars().count();
    if total < 3 {
        return false;
    }
    // Count characters that are either non-printable ASCII (< 0x20, excluding
    // normal whitespace) or non-ASCII (> 0x7F).  Both are reliable indicators
    // of raw glyph bytes misinterpreted as Latin-1.
    let bad = text.chars().filter(|&c| {
        let u = c as u32;
        (u < 0x20 && c != '\t' && c != '\n' && c != '\r') || u > 0x7F
    }).count();
    (bad as f64 / total as f64) > 0.40
}

/// Remove individual whitespace-delimited tokens that look like font-encoding
/// garbage from `text`, preserving the surrounding valid content.
fn strip_garbled_tokens(text: &str) -> String {
    text.split_whitespace()
        .filter(|t| !looks_garbled(t))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Split extracted page text into paragraphs. Blank lines delimit paragraphs;
/// within a paragraph, soft line breaks are joined with spaces.
fn split_paragraphs(text: &str) -> Vec<String> {
    let normalized = text.replace('\r', "\n");
    normalized
        .split("\n\n")
        .map(|p| {
            p.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

// A GlyphMap maps a 2-byte CID glyph index to its Unicode character, built
// from a font's /ToUnicode CMap stream.
type GlyphMap = std::collections::HashMap<u16, char>;
// Keyed by font resource name bytes (e.g. b"C2_0"), as they appear in the Tf
// operator and in the /Resources /Font dictionary.
type FontCmaps = std::collections::HashMap<Vec<u8>, GlyphMap>;

/// Collect text from every Form XObject on a page.  For each Form XObject the
/// function loads the per-font ToUnicode CMaps so CID-encoded strings (2-byte
/// glyph IDs produced by Type0/Identity-H fonts) are decoded to real Unicode
/// rather than treated as garbage and filtered out.
fn form_xobject_text(doc: &Document, page_id: lopdf::ObjectId) -> String {
    use lopdf::content::Content;

    let mut result = String::new();

    // Collect Form XObject IDs from the page's /Resources /XObject dict.
    // Gathering IDs (Copy values) first means we never hold a live borrow on
    // `doc` across the processing loop below.
    let form_ids: Vec<lopdf::ObjectId> = {
        let xobjects: &lopdf::Dictionary = {
            let Ok(page_obj) = doc.get_object(page_id) else {
                return result;
            };
            let Ok(page_dict) = page_obj.as_dict() else {
                return result;
            };
            let Ok(res_val) = page_dict.get(b"Resources") else {
                return result;
            };
            let Ok((_, res_obj)) = doc.dereference(res_val) else {
                return result;
            };
            let Ok(res_dict) = res_obj.as_dict() else {
                return result;
            };
            let Ok(xobj_val) = res_dict.get(b"XObject") else {
                return result;
            };
            let Ok((_, xobj_obj)) = doc.dereference(xobj_val) else {
                return result;
            };
            let Ok(d) = xobj_obj.as_dict() else {
                return result;
            };
            d
        };
        xobjects
            .iter()
            .filter_map(|(_, v)| v.as_reference().ok())
            .filter(|&id| {
                matches!(
                    doc.get_object(id)
                        .ok()
                        .and_then(|o| if let Object::Stream(s) = o {
                            s.dict.get(b"Subtype").ok()?.as_name().ok()
                        } else {
                            None
                        }),
                    Some(n) if n == b"Form"
                )
            })
            .collect()
    };

    for form_id in form_ids {
        // Phase 1 — build per-font ToUnicode CMaps.  Resolved into an owned
        // HashMap so document borrows are released before Phase 2.
        let cmaps = load_form_font_cmaps(doc, form_id);

        // Phase 2 — decompress and parse the Form XObject's content stream.
        let content_bytes: Vec<u8> = {
            let Ok(Object::Stream(s)) = doc.get_object(form_id) else {
                continue;
            };
            decompress_stream_bytes(s)
        };

        let Ok(content) = Content::decode(&content_bytes) else {
            continue;
        };
        let text = text_from_content_ops(&content.operations, &cmaps);
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            result.push('\n');
            result.push_str(trimmed);
        }
    }

    result
}

/// Load ToUnicode CMaps for every font in a Form XObject's /Resources /Font
/// dict.  Returns an empty map when the Form has no font resources.
fn load_form_font_cmaps(doc: &Document, form_id: lopdf::ObjectId) -> FontCmaps {
    let mut cmaps = FontCmaps::new();

    // Resolve the Form XObject's font dictionary.  Uses a closure so each
    // early-exit path returns None cleanly, avoiding `let…else` divergence
    // issues with the intermediate references.
    let font_dict: Option<&lopdf::Dictionary> = (|| {
        let obj = doc.get_object(form_id).ok()?;
        let stream = if let Object::Stream(s) = obj { s } else { return None };
        let res_val = stream.dict.get(b"Resources").ok()?;
        let (_, res_obj) = doc.dereference(res_val).ok()?;
        let res_dict = res_obj.as_dict().ok()?;
        let font_val = res_dict.get(b"Font").ok()?;
        let (_, font_obj) = doc.dereference(font_val).ok()?;
        font_obj.as_dict().ok()
    })();

    let Some(font_dict) = font_dict else {
        return cmaps;
    };

    for (font_name, font_ref) in font_dict.iter() {
        let Ok(font_id) = font_ref.as_reference() else {
            continue;
        };
        let Ok(font_obj) = doc.get_object(font_id) else {
            continue;
        };
        let Ok(fd) = font_obj.as_dict() else {
            continue;
        };
        let Ok(tu_val) = fd.get(b"ToUnicode") else {
            continue;
        };
        let Ok(tu_id) = tu_val.as_reference() else {
            continue;
        };
        let Ok(Object::Stream(tu)) = doc.get_object(tu_id) else {
            continue;
        };
        let gmap = parse_to_unicode_cmap(&decompress_stream_bytes(tu));
        if !gmap.is_empty() {
            cmaps.insert(font_name.to_vec(), gmap);
        }
    }
    cmaps
}

/// Return the decoded (decompressed) bytes of a PDF stream's content.
/// Handles FlateDecode; falls back to raw bytes for uncompressed or unknown
/// filters.
fn decompress_stream_bytes(s: &lopdf::Stream) -> Vec<u8> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;
    let uses_flate = s
        .dict
        .get(b"Filter")
        .ok()
        .or_else(|| s.dict.get(b"F").ok())
        .map(|f| match f {
            Object::Name(n) => n == b"FlateDecode" || n == b"Fl",
            Object::Array(a) => matches!(
                a.first(),
                Some(Object::Name(n)) if n == b"FlateDecode" || n == b"Fl"
            ),
            _ => false,
        })
        .unwrap_or(false);
    if uses_flate {
        let mut decoded = Vec::new();
        if ZlibDecoder::new(s.content.as_slice())
            .read_to_end(&mut decoded)
            .is_ok()
            && !decoded.is_empty()
        {
            return decoded;
        }
    }
    s.content.clone()
}

/// Parse a `/ToUnicode` CMap stream and return a mapping from 2-byte CID
/// glyph indices to Unicode `char` values.  Handles `beginbfchar`/`endbfchar`
/// and `beginbfrange`/`endbfrange` sections.
fn parse_to_unicode_cmap(data: &[u8]) -> GlyphMap {
    let text = String::from_utf8_lossy(data);
    let mut map = GlyphMap::new();
    let mut mode = 0u8; // 0=none, 1=bfchar, 2=bfrange

    for line in text.lines() {
        let line = line.trim();
        // The count prefix ("N beginbfchar") means we test with ends_with, not ==.
        if line.ends_with("beginbfchar") {
            mode = 1;
            continue;
        } else if line == "endbfchar" {
            mode = 0;
            continue;
        } else if line.ends_with("beginbfrange") {
            mode = 2;
            continue;
        } else if line == "endbfrange" {
            mode = 0;
            continue;
        }
        match mode {
            1 => {
                // <srcGlyphId> <dstUnicode>
                let hexes = cmap_hex_tokens(line);
                if hexes.len() >= 2 {
                    if let (Some(src), Some(dst)) = (
                        hex_to_u16(hexes[0]),
                        hex_to_char(hexes[1]),
                    ) {
                        if dst != '\u{FFFF}' {
                            map.insert(src, dst);
                        }
                    }
                }
            }
            2 => {
                // <srcLo> <srcHi> <dstBase>  — contiguous range
                let hexes = cmap_hex_tokens(line);
                if hexes.len() >= 3 {
                    if let (Some(lo), Some(hi), Some(base)) = (
                        hex_to_u16(hexes[0]),
                        hex_to_u16(hexes[1]),
                        hex_to_u32(hexes[2]),
                    ) {
                        for (i, gid) in (lo..=hi).enumerate() {
                            if let Some(c) = char::from_u32(base + i as u32) {
                                map.insert(gid, c);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    map
}

/// Extract hex digit runs from inside `<...>` tokens on a CMap line.
fn cmap_hex_tokens(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(open) = rest.find('<') {
        rest = &rest[open + 1..];
        if let Some(close) = rest.find('>') {
            let hex = &rest[..close];
            if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                out.push(hex);
            }
            rest = &rest[close + 1..];
        } else {
            break;
        }
    }
    out
}

fn hex_to_u16(hex: &str) -> Option<u16> {
    if hex.len() != 4 {
        return None;
    }
    u16::from_str_radix(hex, 16).ok()
}

fn hex_to_u32(hex: &str) -> Option<u32> {
    u32::from_str_radix(hex, 16).ok()
}

fn hex_to_char(hex: &str) -> Option<char> {
    char::from_u32(u32::from_str_radix(hex, 16).ok()?)
}

/// Walk content-stream operations and extract all text inside BT…ET blocks.
/// Font changes (`Tf`) are tracked so each string is decoded with the correct
/// ToUnicode CMap when one is available.
fn text_from_content_ops(ops: &[lopdf::content::Operation], cmaps: &FontCmaps) -> String {
    let mut buf = String::new();
    let mut in_text = false;
    let mut cur_cmap: Option<&GlyphMap> = None;

    for op in ops {
        match op.operator.as_str() {
            "BT" => in_text = true,
            "ET" => {
                in_text = false;
                if !buf.ends_with('\n') {
                    buf.push('\n');
                }
            }
            // Track font changes: operands are [Name fontKey, Number size]
            "Tf" => {
                cur_cmap = op.operands.first().and_then(|o| {
                    if let Object::Name(n) = o {
                        cmaps.get(n.as_slice())
                    } else {
                        None
                    }
                });
            }
            "Tj" | "'" if in_text => {
                if let Some(Object::String(b, _)) = op.operands.first() {
                    buf.push_str(&decode_pdf_string(b, cur_cmap));
                }
            }
            "\"" if in_text => {
                if let Some(Object::String(b, _)) = op.operands.get(2) {
                    buf.push_str(&decode_pdf_string(b, cur_cmap));
                }
            }
            "TJ" if in_text => {
                if let Some(Object::Array(arr)) = op.operands.first() {
                    for item in arr {
                        if let Object::String(b, _) = item {
                            buf.push_str(&decode_pdf_string(b, cur_cmap));
                        }
                    }
                }
            }
            "Td" | "TD" | "T*" if in_text => {
                if !buf.ends_with('\n') && !buf.ends_with(' ') {
                    buf.push('\n');
                }
            }
            _ => {}
        }
    }
    buf
}

/// Decode a PDF string's raw bytes to a Rust String.
///
/// If `cmap` is provided (a ToUnicode map for the current Type0/CID font),
/// each pair of bytes is treated as a big-endian 2-byte glyph ID and looked up
/// in the map.  Without a CMap, falls back to UTF-16BE BOM detection, then
/// Latin-1 / PDFDoc encoding.
fn decode_pdf_string(bytes: &[u8], cmap: Option<&GlyphMap>) -> String {
    // CID path: use the font's ToUnicode map
    if let Some(map) = cmap {
        if bytes.len() % 2 == 0 {
            let mut out = String::with_capacity(bytes.len() / 2);
            for chunk in bytes.chunks_exact(2) {
                let gid = u16::from_be_bytes([chunk[0], chunk[1]]);
                if let Some(&c) = map.get(&gid) {
                    out.push(c);
                }
                // Unmapped glyph IDs are silently skipped (they're typically
                // spaces or ligatures whose visual role is already covered by
                // adjacent literal strings from the same text run).
            }
            if !out.is_empty() {
                return out;
            }
        }
    }
    // Fallback: explicit UTF-16BE BOM
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    // Fallback: Latin-1 / PDFDoc encoding
    bytes.iter().map(|&b| b as char).collect()
}


// ── OCR via the tesseract CLI ──────────────────────────────────────────────────

pub(super) fn tesseract_available() -> bool {
    Command::new("tesseract")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// OCR a page by extracting its largest embedded image and running tesseract.
/// Returns None when tesseract is unavailable, no usable image is found, or the
/// run fails. (Rasterizing vector/text pages would need a PDF rasterizer, which
/// is out of scope; this covers the common "scanned page = one image" case.)
fn ocr_page(doc: &Document, pageno: u32, lang: &str) -> Option<String> {
    if !tesseract_available() {
        return None;
    }
    let jpeg = largest_page_jpeg(doc, pageno)?;

    let dir = std::env::temp_dir();
    let img_path = dir.join(format!("aipdf_ocr_{}_{}.jpg", std::process::id(), pageno));
    std::fs::write(&img_path, &jpeg).ok()?;

    let output = Command::new("tesseract")
        .arg(&img_path)
        .arg("stdout")
        .arg("-l")
        .arg(lang)
        .output()
        .ok();
    std::fs::remove_file(&img_path).ok();

    let output = output?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Find the largest JPEG (DCTDecode) image XObject referenced by a page. JPEG
/// streams store their raw bytes directly, so we can hand them to tesseract as a
/// `.jpg`. Other filters (CCITT/JBIG2/Flate) are not handled here.
fn largest_page_jpeg(doc: &Document, pageno: u32) -> Option<Vec<u8>> {
    let page_id = *doc.get_pages().get(&pageno)?;
    let (resources, _) = doc.get_page_resources(page_id).ok()?;
    let xobjects = resources
        .and_then(|d| d.get(b"XObject").ok())
        .and_then(|o| o.as_dict().ok())?;

    let mut best: Option<Vec<u8>> = None;
    for (_name, obj) in xobjects.iter() {
        let Ok(id) = obj.as_reference() else { continue };
        let Ok(Object::Stream(s)) = doc.get_object(id) else {
            continue;
        };
        let is_image =
            s.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Image".as_ref());
        let is_jpeg = matches!(
            s.dict.get(b"Filter").ok().and_then(|o| o.as_name().ok()),
            Some(f) if f == b"DCTDecode"
        );
        if is_image && is_jpeg && best.as_ref().map_or(true, |b| s.content.len() > b.len()) {
            best = Some(s.content.clone());
        }
    }
    best
}
