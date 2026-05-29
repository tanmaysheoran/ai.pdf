use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::collections::BTreeMap;
use std::io::Write;

use super::Font;

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
