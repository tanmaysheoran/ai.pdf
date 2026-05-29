//! Embedded CID/Type0 TrueType font support.
//!
//! The visible PDF layer is drawn with a real embedded TrueType font using the
//! `Identity-H` encoding. This means the content stream shows text as 2-byte
//! glyph IDs (GIDs) rather than Latin-1 byte strings, so any Unicode codepoint
//! the font has a glyph for renders correctly — and a `ToUnicode` CMap keeps
//! copy/paste and text extraction working.
//!
//! To avoid the self-referential-borrow problem of holding a `ttf_parser::Face`
//! (which borrows the font bytes) next to those bytes, we parse the face once at
//! construction and precompute everything we need into owned tables: the cmap
//! (codepoint → GID) and per-glyph advance widths. The raw bytes are retained
//! only to be embedded as the `FontFile2` stream.

use crate::{AipdfError, Result};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::Path;

/// The default embedded font (DejaVu Sans — freely redistributable; covers
/// Latin, Latin Extended, Greek, Cyrillic, and more). Point `--font` at a Noto
/// CJK / RTL face to cover those scripts; the embedding machinery below is
/// glyph-agnostic and works with any TrueType `glyf` font.
const DEJAVU_SANS: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");

/// A parsed font with owned lookup tables, ready for layout + embedding.
#[derive(Clone)]
pub struct Font {
    raw: Vec<u8>,
    /// Glyph-space units per em (e.g. 2048 for DejaVu Sans).
    units_per_em: f32,
    /// Unicode codepoint → glyph id.
    cmap: HashMap<u32, u16>,
    /// Per-glyph horizontal advance, in font units (indexed by GID).
    advances: Vec<u16>,
    // Descriptor metrics, scaled to the conventional 1000-units-per-em space.
    ascent: i32,
    descent: i32,
    cap_height: i32,
    bbox: (i32, i32, i32, i32),
    italic_angle: f32,
    postscript_name: String,
}

impl Font {
    /// Load the built-in DejaVu Sans face.
    pub fn default_font() -> Font {
        // The vendored font is known-good, so parsing cannot fail in practice.
        Self::from_bytes(DEJAVU_SANS.to_vec()).expect("vendored DejaVuSans.ttf must parse")
    }

    /// Load a TrueType font from disk (used for `--font`, e.g. a CJK face).
    pub fn from_path(path: &Path) -> Result<Font> {
        let bytes = std::fs::read(path)
            .map_err(|e| AipdfError::InvalidXml(format!("cannot read font {path:?}: {e}")))?;
        Self::from_bytes(bytes)
    }

    fn from_bytes(raw: Vec<u8>) -> Result<Font> {
        let face = ttf_parser::Face::parse(&raw, 0)
            .map_err(|e| AipdfError::InvalidXml(format!("invalid TrueType font: {e}")))?;

        let upem = face.units_per_em();
        if upem == 0 {
            return Err(AipdfError::InvalidXml("font has zero units_per_em".into()));
        }
        let upem_f = upem as f32;
        let to_1000 = |v: i32| (v as f32 * 1000.0 / upem_f).round() as i32;

        // Precompute the Unicode cmap so per-char lookups don't need the face.
        let mut cmap: HashMap<u32, u16> = HashMap::new();
        if let Some(table) = face.tables().cmap {
            for subtable in table.subtables {
                if !subtable.is_unicode() {
                    continue;
                }
                subtable.codepoints(|cp| {
                    if let Some(gid) = subtable.glyph_index(cp) {
                        cmap.entry(cp).or_insert(gid.0);
                    }
                });
            }
        }

        // Precompute advances for every glyph.
        let n = face.number_of_glyphs();
        let mut advances = Vec::with_capacity(n as usize);
        for gid in 0..n {
            advances.push(
                face.glyph_hor_advance(ttf_parser::GlyphId(gid))
                    .unwrap_or(0),
            );
        }

        let bb = face.global_bounding_box();
        let cap = face.capital_height().unwrap_or(face.ascender()) as i32;
        let raw_name = face
            .names()
            .into_iter()
            .find(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
            .and_then(|n| n.to_string())
            .unwrap_or_else(|| "EmbeddedFont".to_string());
        // PostScript names must be plain ASCII without spaces/delimiters.
        let postscript_name: String = raw_name
            .chars()
            .filter(|c| c.is_ascii_graphic() && !"()<>[]{}/% ".contains(*c))
            .collect();

        // Materialize all face-derived values before dropping the borrow on `raw`.
        let ascent = to_1000(face.ascender() as i32);
        let descent = to_1000(face.descender() as i32);
        let cap_height = to_1000(cap);
        let bbox = (
            to_1000(bb.x_min as i32),
            to_1000(bb.y_min as i32),
            to_1000(bb.x_max as i32),
            to_1000(bb.y_max as i32),
        );
        let italic_angle = face.italic_angle();
        drop(face);

        Ok(Font {
            raw,
            units_per_em: upem_f,
            cmap,
            advances,
            ascent,
            descent,
            cap_height,
            bbox,
            italic_angle,
            postscript_name: if postscript_name.is_empty() {
                "EmbeddedFont".to_string()
            } else {
                postscript_name
            },
        })
    }

    /// Glyph id for a codepoint (0 / `.notdef` if the font lacks it).
    pub fn glyph(&self, c: char) -> u16 {
        self.cmap.get(&(c as u32)).copied().unwrap_or(0)
    }

    /// Advance width of a glyph in the 1000-units-per-em PDF glyph space.
    pub fn advance_1000(&self, gid: u16) -> u32 {
        let raw = *self.advances.get(gid as usize).unwrap_or(&0) as f32;
        (raw * 1000.0 / self.units_per_em).round() as u32
    }

    /// Physical width of `text` at `size` points.
    pub fn text_width(&self, text: &str, size: f32) -> f32 {
        text.chars()
            .map(|c| {
                let gid = self.glyph(c);
                *self.advances.get(gid as usize).unwrap_or(&0) as f32 * size / self.units_per_em
            })
            .sum()
    }

    /// Shape `text` into a sequence of (glyph id, source char) pairs. Chars the
    /// font has no glyph for map to GID 0 and are still recorded so callers can
    /// account for them; renderers typically skip GID 0.
    pub fn shape(&self, text: &str) -> Vec<(u16, char)> {
        text.chars().map(|c| (self.glyph(c), c)).collect()
    }
}

/// Accumulates the glyphs actually used across a document so we can emit a
/// compact `/W` widths array and a `/ToUnicode` CMap covering only those glyphs.
#[derive(Default)]
pub struct GlyphSet {
    used: BTreeMap<u16, char>,
}

impl GlyphSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Encode `text` into a content-stream hex string of 2-byte GIDs, recording
    /// each used glyph. GID 0 (missing glyph) is skipped so we don't paint
    /// `.notdef` boxes for unsupported codepoints.
    pub fn encode_hex(&mut self, font: &Font, text: &str) -> String {
        let mut out = String::new();
        for (gid, ch) in font.shape(text) {
            if gid == 0 {
                continue;
            }
            self.used.insert(gid, ch);
            out.push_str(&format!("{gid:04X}"));
        }
        out
    }

    #[allow(dead_code)] // used by tests + external callers
    pub fn is_empty(&self) -> bool {
        self.used.is_empty()
    }
}

// ── PDF object payloads ─────────────────────────────────────────────────────
//
// These build the byte/string bodies for the font object graph. The caller
// (an Assembler) allocates object ids and stitches the references together:
//
//   Type0 ──/DescendantFonts──▶ CIDFontType2 ──/FontDescriptor──▶ descriptor
//     │                                                              │
//     └──/ToUnicode──▶ CMap stream                    /FontFile2 ◀───┘

/// zlib-compress bytes for a `/FlateDecode` stream.
pub fn flate(bytes: &[u8]) -> Vec<u8> {
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(bytes).expect("zlib write to Vec cannot fail");
    enc.finish().expect("zlib finish to Vec cannot fail")
}

/// The compressed `FontFile2` payload and its uncompressed length (`/Length1`).
pub fn font_file2(font: &Font) -> (Vec<u8>, usize) {
    (flate(&font.raw), font.raw.len())
}

/// `/FontDescriptor` dictionary body referencing the embedded `FontFile2`.
pub fn descriptor_dict(font: &Font, font_file2_id: usize) -> String {
    let (x0, y0, x1, y1) = font.bbox;
    // Nonsymbolic, since we provide a ToUnicode map and a Unicode cmap.
    let flags = 32;
    format!(
        "<< /Type /FontDescriptor /FontName /{name} /Flags {flags} \
/FontBBox [{x0} {y0} {x1} {y1}] /ItalicAngle {ia:.1} /Ascent {asc} /Descent {desc} \
/CapHeight {cap} /StemV 80 /FontFile2 {ff} 0 R >>",
        name = font.postscript_name,
        ia = font.italic_angle,
        asc = font.ascent,
        desc = font.descent,
        cap = font.cap_height,
        ff = font_file2_id,
    )
}

/// `/W` array body (e.g. `[3[600] 7[722 333]]`) for the used glyphs only.
fn w_array(font: &Font, used: &BTreeMap<u16, char>) -> String {
    // Emit consecutive GID runs as `start [w w w ...]`.
    let mut out = String::from("[");
    let gids: Vec<u16> = used.keys().copied().collect();
    let mut i = 0;
    while i < gids.len() {
        let start = gids[i];
        let mut widths = vec![font.advance_1000(start)];
        let mut j = i + 1;
        while j < gids.len() && gids[j] == gids[j - 1] + 1 {
            widths.push(font.advance_1000(gids[j]));
            j += 1;
        }
        let body: Vec<String> = widths.iter().map(|w| w.to_string()).collect();
        out.push_str(&format!("{start}[{}]", body.join(" ")));
        i = j;
    }
    out.push(']');
    out
}

/// `CIDFontType2` (descendant) dictionary body.
pub fn cidfont_dict(font: &Font, descriptor_id: usize, used: &BTreeMap<u16, char>) -> String {
    format!(
        "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{name} \
/CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
/FontDescriptor {desc} 0 R /CIDToGIDMap /Identity /DW 1000 /W {w} >>",
        name = font.postscript_name,
        desc = descriptor_id,
        w = w_array(font, used),
    )
}

/// `Type0` composite font dictionary body.
pub fn type0_dict(font: &Font, cidfont_id: usize, tounicode_id: usize) -> String {
    format!(
        "<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H \
/DescendantFonts [{cid} 0 R] /ToUnicode {tu} 0 R >>",
        name = font.postscript_name,
        cid = cidfont_id,
        tu = tounicode_id,
    )
}

/// `/ToUnicode` CMap stream contents mapping each used GID → UTF-16BE.
pub fn tounicode_cmap(used: &BTreeMap<u16, char>) -> Vec<u8> {
    let mut s = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let entries: Vec<(u16, char)> = used.iter().map(|(g, c)| (*g, *c)).collect();
    for chunk in entries.chunks(100) {
        s.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (gid, ch) in chunk {
            s.push_str(&format!("<{gid:04X}> <{}>\n", utf16be_hex(*ch)));
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s.into_bytes()
}

fn utf16be_hex(c: char) -> String {
    let mut buf = [0u16; 2];
    c.encode_utf16(&mut buf)
        .iter()
        .map(|u| format!("{u:04X}"))
        .collect()
}

/// Expose the used-glyph map for assembly helpers.
impl GlyphSet {
    pub fn used(&self) -> &BTreeMap<u16, char> {
        &self.used
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_font_has_unicode_glyphs() {
        let f = Font::default_font();
        // ASCII, Latin-1 accented, Cyrillic, and Greek are all in DejaVu Sans.
        for c in ['A', 'é', 'ñ', 'ü', 'Ж', 'Ω'] {
            assert_ne!(f.glyph(c), 0, "missing glyph for {c:?}");
        }
        assert!(f.text_width("Hello", 12.0) > 0.0);
    }

    #[test]
    fn encode_records_glyphs_and_skips_notdef() {
        let f = Font::default_font();
        let mut gs = GlyphSet::new();
        let hex = gs.encode_hex(&f, "Aé");
        assert_eq!(hex.len(), 8, "two glyphs => 8 hex chars: {hex}");
        assert_eq!(gs.used().len(), 2);
        assert!(!gs.is_empty());
    }

    #[test]
    fn font_file2_compresses_and_tounicode_maps() {
        let f = Font::default_font();
        let (compressed, len1) = font_file2(&f);
        assert!(compressed.len() < len1, "flate should shrink the font");
        let mut gs = GlyphSet::new();
        gs.encode_hex(&f, "Ω");
        let cmap = String::from_utf8(tounicode_cmap(gs.used())).unwrap();
        // Ω is U+03A9.
        assert!(cmap.contains("03A9"), "ToUnicode must map back to U+03A9");
        assert!(cmap.contains("beginbfchar"));
    }
}
