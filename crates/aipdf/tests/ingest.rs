//! Ingest an ordinary (non-aipdf) PDF and verify a semantic layer is attached,
//! extractable, and that the original visible text is preserved.

use aipdf::{extract_semantic_xml, ingest_pdf, inspect_pdf, IngestOptions, OcrMode};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};

fn plain_text_pdf(text: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 24.into()]),
            Operation::new("Td", vec![72.into(), 700.into()]),
            Operation::new("Tj", vec![Object::string_literal(text)]),
            Operation::new("ET", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => resources_id,
    });
    let pages = dictionary! {
        "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    buf
}

#[test]
fn ingest_attaches_extractable_semantic_layer() {
    let original = plain_text_pdf("Hello ingest world");
    // The source PDF has no semantic layer yet.
    assert!(!inspect_pdf(&original).has_semantic_layer);

    let out = ingest_pdf(
        &original,
        &IngestOptions {
            ocr: OcrMode::Never,
            lang: "eng".into(),
        },
    )
    .expect("ingest");

    // The ingested file now has a detectable, extractable semantic layer...
    let report = inspect_pdf(&out);
    assert!(report.has_semantic_layer, "ingested file must be detected");
    let xml = extract_semantic_xml(&out).expect("extract");
    assert!(
        xml.contains("Hello ingest world"),
        "semantic layer must contain page text; got: {xml}"
    );
    assert!(xml.contains(r#"id="ingested""#));

    // ...and the original visible text is still rendered in the PDF.
    let doc = Document::load_mem(&out).unwrap();
    let visible = doc.extract_text(&[1]).unwrap();
    assert!(visible.contains("Hello ingest world"));
}

#[test]
fn force_ocr_without_tesseract_errors() {
    // CI here has no tesseract; force mode must fail loudly rather than silently
    // produce an empty layer. (If tesseract IS installed this test is skipped.)
    let has_tesseract = std::process::Command::new("tesseract")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if has_tesseract {
        return;
    }
    let original = plain_text_pdf("scanned-looking page");
    let err = ingest_pdf(
        &original,
        &IngestOptions {
            ocr: OcrMode::Force,
            lang: "eng".into(),
        },
    );
    assert!(err.is_err(), "force OCR without tesseract should error");
}
