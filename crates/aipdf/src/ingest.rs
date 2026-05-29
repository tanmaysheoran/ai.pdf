//! Ingest an existing PDF: extract its text (with an optional OCR fallback for
//! scanned pages) into a semantic XML layer, then attach that layer to the
//! *original* PDF so its visuals are preserved byte-for-byte where possible.
//!
//! Text extraction uses `lopdf`. OCR shells out to the `tesseract` CLI when
//! available; there is no good pure-Rust OCR, so when `tesseract` is missing we
//! degrade gracefully (auto mode keeps whatever text we have; force mode errors
//! with an install hint).

use crate::{pdf::brotli_compress, security::sanitize_xml, source::xml_escape, xml::validate_xml};
use crate::{AipdfError, Result};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use sha2::{Digest, Sha256};
use std::process::Command;

const SEMANTIC_FILENAME: &str = "aipdf-semantic.xml.br";
/// Below this many extractable chars, a page is treated as "scanned" and is a
/// candidate for OCR (in auto mode).
const TEXT_THRESHOLD: usize = 16;

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

    attach_semantic_layer(&mut doc, &xml)?;

    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| AipdfError::Pdf(format!("cannot serialize PDF: {e}")))?;
    Ok(out)
}

// ── Text / OCR extraction → semantic XML ──────────────────────────────────────

fn extract_semantic_xml(doc: &Document, opts: &IngestOptions) -> Result<String> {
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

fn tesseract_available() -> bool {
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

// ── Attach the semantic layer to an existing document ──────────────────────────

fn attach_semantic_layer(doc: &mut Document, xml: &str) -> Result<()> {
    let compressed = brotli_compress(xml.as_bytes())?;
    let compressed_len = compressed.len();
    let checksum = Sha256::digest(&compressed).to_vec();

    // EmbeddedFile stream (no PDF /Filter; the bytes are the Brotli payload).
    let ef = Stream::new(
        dictionary! {
            "Type" => Object::Name(b"EmbeddedFile".to_vec()),
            "Subtype" => Object::Name(b"application#aipdf+xml+br".to_vec()),
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
        xmp_metadata(xml.len(), compressed_len).into_bytes(),
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

fn xmp_metadata(xml_bytes: usize, compressed_bytes: usize) -> String {
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
      <aipdf:SourceFormat>ingested-pdf</aipdf:SourceFormat>
      <aipdf:SemanticXmlBytes>{xml_bytes}</aipdf:SemanticXmlBytes>
      <aipdf:SemanticCompressedBytes>{compressed_bytes}</aipdf:SemanticCompressedBytes>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#
    )
}
