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

/// Below this many extractable chars, a page is treated as "scanned" and is a
/// candidate for OCR (in auto mode).
const TEXT_THRESHOLD: usize = 16;

pub(super) fn extract_semantic_xml(doc: &Document, opts: &IngestOptions) -> Result<String> {
    let pages = doc.get_pages();
    let mut blocks: Vec<String> = Vec::new();
    let mut bid = 1usize;

    for (pageno, _) in pages {
        let embedded = if opts.ocr == OcrMode::Force {
            String::new()
        } else {
            doc.extract_text(&[pageno]).unwrap_or_default()
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
