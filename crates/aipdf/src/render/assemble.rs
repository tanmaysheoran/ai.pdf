use crate::source::xml_escape;
use sha2::{Digest, Sha256};

use super::SEMANTIC_FILENAME;

// ── PDF assembly ──────────────────────────────────────────────────────────────

pub(super) struct Assembler {
    pub(super) objects: Vec<Vec<u8>>,
}

impl Assembler {
    pub(super) fn new() -> Self {
        Self { objects: Vec::new() }
    }

    pub(super) fn add(&mut self, bytes: Vec<u8>) -> usize {
        let id = self.objects.len() + 1;
        self.objects.push(bytes);
        id
    }

    pub(super) fn reserve(&mut self) -> usize {
        self.add(Vec::new())
    }

    pub(super) fn set(&mut self, id: usize, bytes: Vec<u8>) {
        self.objects[id - 1] = bytes;
    }

    pub(super) fn build(self, root_id: usize, title: &str) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");
        let mut offsets = vec![0usize];
        for (i, obj) in self.objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            out.extend_from_slice(obj);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref_offset = out.len();
        let total = self.objects.len() + 1;
        out.extend_from_slice(format!("xref\n0 {total}\n").as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for o in offsets.iter().skip(1) {
            out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        let esc_title = pdf_str(title);
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {total} /Root {root_id} 0 R /Info << /Title ({esc_title}) /Producer (aipdf) >> >>\nstartxref\n{xref_offset}\n%%EOF\n"
            )
            .as_bytes(),
        );
        out
    }
}

pub(super) fn stream_obj(bytes: &[u8], dict: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let dict = dict.trim_end_matches(">>").trim();
    out.extend_from_slice(format!("{dict} /Length {} >>\nstream\n", bytes.len()).as_bytes());
    out.extend_from_slice(bytes);
    out.extend_from_slice(b"\nendstream");
    out
}

pub(super) fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub(super) fn xmp_metadata(title: &str, xml_bytes: usize, compressed_bytes: usize) -> String {
    format!(
        r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about=""
      xmlns:dc="http://purl.org/dc/elements/1.1/"
      xmlns:aipdf="https://aipdf.org/ns/1.0/">
      <dc:title><rdf:Alt><rdf:li xml:lang="x-default">{}</rdf:li></rdf:Alt></dc:title>
      <aipdf:Version>1.0</aipdf:Version>
      <aipdf:SemanticFile>{SEMANTIC_FILENAME}</aipdf:SemanticFile>
      <aipdf:SemanticEncoding>brotli</aipdf:SemanticEncoding>
      <aipdf:SemanticLayerPresent>true</aipdf:SemanticLayerPresent>
      <aipdf:SemanticXmlBytes>{xml_bytes}</aipdf:SemanticXmlBytes>
      <aipdf:SemanticCompressedBytes>{compressed_bytes}</aipdf:SemanticCompressedBytes>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#,
        xml_escape(title)
    )
}

// ── PDF string encoding (used only for /Info and dict literals) ────────────────

pub(super) fn pdf_str(input: &str) -> String {
    let mut out = String::new();
    for c in input.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\r' | '\n' => out.push(' '),
            c if (c as u32) < 32 => out.push(' '),
            c if (c as u32) > 126 => out.push('?'),
            c => out.push(c),
        }
    }
    out
}
