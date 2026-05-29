use crate::font::{self, Font, GlyphSet};

use super::meta::{hex_sha256, pdf_string, xmp_metadata};
use super::{SEMANTIC_FILENAME, SEMANTIC_SUBTYPE};

pub(super) fn visible_text_from_xml(xml: &str) -> String {
    crate::xml::get_reading_order(xml)
        .map(|blocks| {
            blocks
                .into_iter()
                .filter(|b| matches!(b.kind.as_str(), "title" | "paragraph" | "caption"))
                .map(|b| b.text)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|_| "AIPDF document".to_string())
}

pub(super) fn write_pdf(title: &str, visible_text: &str, xml: &str, compressed: &[u8], font: &Font) -> Vec<u8> {
    // Encode the visible text as embedded-font glyph IDs so non-ASCII survives.
    let mut glyphs = GlyphSet::new();
    let mut content = String::from("BT\n/F1 12 Tf\n72 740 Td\n14 TL\n");
    for line in visible_text.lines().take(45) {
        let h = glyphs.encode_hex(font, line);
        content.push_str(&format!("<{h}> Tj\nT*\n"));
    }
    content.push_str("ET\n");
    let used = glyphs.used();

    let xmp = xmp_metadata(title, xml.len(), compressed.len());
    let checksum = hex_sha256(compressed);
    let escaped_title = pdf_string(title);
    let producer_note = pdf_string(
        "AIPDF semantic layer present: extract aipdf-semantic.xml.br, Brotli-decompress, parse XML.",
    );

    // Fixed object layout (single page). Font objects 9–13 are wired into the
    // page's /Resources via the Type0 font (object 13).
    let (ff_bytes, len1) = font::font_file2(font);
    let objects: Vec<Vec<u8>> = vec![
        // 1 Catalog
        b"<< /Type /Catalog /Pages 2 0 R /Metadata 6 0 R /Names << /EmbeddedFiles 7 0 R >> /AF [8 0 R] >>".to_vec(),
        // 2 Pages
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        // 3 Page (Type0 font is object 14)
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 14 0 R >> >> /Contents 5 0 R >>".to_vec(),
        // 4 (unused legacy font slot kept to preserve numbering) -> empty dict
        b"<< >>".to_vec(),
        // 5 visible content stream
        stream_object(content.as_bytes(), "<< >>"),
        // 6 XMP metadata
        stream_object(xmp.as_bytes(), "<< /Type /Metadata /Subtype /XML >>"),
        // 7 EmbeddedFiles names
        b"<< /Names [(aipdf-semantic.xml.br) 8 0 R] >>".to_vec(),
        // 8 Filespec
        format!(
            "<< /Type /Filespec /F ({SEMANTIC_FILENAME}) /UF ({SEMANTIC_FILENAME}) /Desc ({escaped_title} semantic XML) /AFRelationship /Data /EF << /F 9 0 R /UF 9 0 R >> >>"
        )
        .into_bytes(),
        // 9 EmbeddedFile (Brotli semantic XML)
        stream_object(
            compressed,
            &format!(
                "<< /Type /EmbeddedFile /Subtype {SEMANTIC_SUBTYPE} /Params << /Size {} /CheckSum <{}> >> >>",
                xml.len(), checksum
            ),
        ),
        // 10 FontFile2
        stream_object(&ff_bytes, &format!("<< /Length1 {len1} /Filter /FlateDecode >>")),
        // 11 FontDescriptor (-> FontFile2 obj 10)
        font::descriptor_dict(font, 10).into_bytes(),
        // 12 CIDFontType2 (-> FontDescriptor obj 11)
        font::cidfont_dict(font, 11, used).into_bytes(),
        // 13 ToUnicode CMap
        stream_object(&font::tounicode_cmap(used), "<< >>"),
        // 14 Type0 (-> CIDFont obj 12, ToUnicode obj 13)
        font::type0_dict(font, 12, 13).into_bytes(),
    ];

    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = vec![0usize];
    for (idx, obj) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", idx + 1).as_bytes());
        pdf.extend_from_slice(obj);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info << /Title ({escaped_title}) /Producer (aipdf prototype) /AIPDFNote ({producer_note}) >> >>\nstartxref\n{}\n%%EOF\n",
            objects.len() + 1,
            xref_offset
        )
        .as_bytes(),
    );
    pdf
}

pub(super) fn stream_object(bytes: &[u8], dict: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let dict = dict.trim_end_matches(">>").trim();
    out.extend_from_slice(format!("{dict} /Length {} >>\nstream\n", bytes.len()).as_bytes());
    out.extend_from_slice(bytes);
    out.extend_from_slice(b"\nendstream");
    out
}
