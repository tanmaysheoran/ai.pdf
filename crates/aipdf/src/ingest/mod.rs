//! Ingest an existing PDF: extract its text (with an optional OCR fallback for
//! scanned pages) into a semantic XML layer, then attach that layer to the
//! *original* PDF so its visuals are preserved byte-for-byte where possible.
//!
//! Text extraction uses `lopdf`. OCR shells out to the `tesseract` CLI when
//! available; there is no good pure-Rust OCR, so when `tesseract` is missing we
//! degrade gracefully (auto mode keeps whatever text we have; force mode errors
//! with an install hint).

mod extract;

use crate::{pdf::brotli_compress, security::sanitize_xml, xml::validate_xml};
use crate::{AipdfError, Result};
use extract::{extract_semantic_xml, tesseract_available};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use sha2::{Digest, Sha256};

const SEMANTIC_FILENAME: &str = "aipdf-semantic.xml.br";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OcrMode {
    /// OCR only pages with little or no extractable text.
    #[default]
    Auto,
    /// Never OCR; use embedded text only.
    Never,
    /// OCR every page, ignoring embedded text.
    Force,
}

#[derive(Debug, Clone)]
pub struct IngestOptions {
    pub ocr: OcrMode,
    /// Tesseract language code(s), e.g. "eng" or "eng+deu".
    pub lang: String,
}

impl Default for IngestOptions {
    fn default() -> Self {
        Self {
            ocr: OcrMode::Auto,
            lang: "eng".to_string(),
        }
    }
}

/// Ingest `pdf_bytes`, returning new PDF bytes that are the original document
/// with an `.ai.pdf` semantic layer attached.
pub fn ingest_pdf(pdf_bytes: &[u8], opts: &IngestOptions) -> Result<Vec<u8>> {
    let mut doc = Document::load_mem(pdf_bytes)
        .map_err(|e| AipdfError::Pdf(format!("cannot parse PDF: {e}")))?;

    if opts.ocr == OcrMode::Force && !tesseract_available() {
        return Err(AipdfError::Pdf(
            "OCR forced but `tesseract` was not found on PATH (install it, e.g. `brew install tesseract`)".into(),
        ));
    }

    let xml = extract_semantic_xml(&doc, opts)?;
    let xml = sanitize_xml(&xml)?;
    validate_xml(&xml)?;

    attach_semantic_layer(&mut doc, &xml, "ingested-pdf")?;

    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| AipdfError::Pdf(format!("cannot serialize PDF: {e}")))?;
    Ok(out)
}

// ── Attach the semantic layer to an existing document ──────────────────────────

/// Attach a Brotli-compressed semantic XML layer to an already-parsed PDF,
/// patching its catalog (`/AF`, `/Metadata`, `/Names /EmbeddedFiles`) while
/// preserving the existing visuals. Shared by `ingest` and the browser render
/// path; `source_format` is recorded in the XMP packet (`aipdf:SourceFormat`).
pub(crate) fn attach_semantic_layer(
    doc: &mut Document,
    xml: &str,
    source_format: &str,
) -> Result<()> {
    let compressed = brotli_compress(xml.as_bytes())?;
    let compressed_len = compressed.len();
    let checksum = Sha256::digest(&compressed).to_vec();

    // EmbeddedFile stream (no PDF /Filter; the bytes are the Brotli payload).
    let ef = Stream::new(
        dictionary! {
            "Type" => Object::Name(b"EmbeddedFile".to_vec()),
            // Unescaped name bytes; lopdf escapes the '/' to #2F on write.
            "Subtype" => Object::Name(b"application/aipdf+xml+br".to_vec()),
            "Params" => dictionary! {
                "Size" => Object::Integer(xml.len() as i64),
                "CheckSum" => Object::String(checksum, StringFormat::Hexadecimal),
            },
        },
        compressed,
    );
    let ef_id = doc.add_object(ef);

    let filespec = dictionary! {
        "Type" => Object::Name(b"Filespec".to_vec()),
        "F" => Object::string_literal(SEMANTIC_FILENAME),
        "UF" => Object::string_literal(SEMANTIC_FILENAME),
        "AFRelationship" => Object::Name(b"Data".to_vec()),
        "EF" => dictionary! {
            "F" => Object::Reference(ef_id),
            "UF" => Object::Reference(ef_id),
        },
    };
    let filespec_id = doc.add_object(filespec);

    let names = dictionary! {
        "Names" => Object::Array(vec![
            Object::string_literal(SEMANTIC_FILENAME),
            Object::Reference(filespec_id),
        ]),
    };
    let names_id = doc.add_object(names);

    let meta = Stream::new(
        dictionary! {
            "Type" => Object::Name(b"Metadata".to_vec()),
            "Subtype" => Object::Name(b"XML".to_vec()),
        },
        xmp_metadata(xml.len(), compressed_len, source_format).into_bytes(),
    );
    let meta_id = doc.add_object(meta);

    // Patch the catalog, preserving its existing entries (/Pages etc.).
    let root_id = doc
        .trailer
        .get(b"Root")
        .map_err(|_| AipdfError::Pdf("missing /Root".into()))?
        .as_reference()
        .map_err(|_| AipdfError::Pdf("bad /Root reference".into()))?;
    let catalog = doc
        .get_object_mut(root_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|_| AipdfError::Pdf("catalog is not a dictionary".into()))?;
    catalog.set("AF", Object::Array(vec![Object::Reference(filespec_id)]));
    catalog.set("Metadata", Object::Reference(meta_id));
    catalog.set(
        "Names",
        dictionary! { "EmbeddedFiles" => Object::Reference(names_id) },
    );
    Ok(())
}

fn xmp_metadata(xml_bytes: usize, compressed_bytes: usize, source_format: &str) -> String {
    format!(
        r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about=""
      xmlns:aipdf="https://aipdf.org/ns/1.0/">
      <aipdf:Version>1.0</aipdf:Version>
      <aipdf:SemanticFile>{SEMANTIC_FILENAME}</aipdf:SemanticFile>
      <aipdf:SemanticEncoding>brotli</aipdf:SemanticEncoding>
      <aipdf:SemanticLayerPresent>true</aipdf:SemanticLayerPresent>
      <aipdf:SourceFormat>{source_format}</aipdf:SourceFormat>
      <aipdf:SemanticXmlBytes>{xml_bytes}</aipdf:SemanticXmlBytes>
      <aipdf:SemanticCompressedBytes>{compressed_bytes}</aipdf:SemanticCompressedBytes>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#
    )
}
