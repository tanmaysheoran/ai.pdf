//! Round-trip and third-party-reassembly survival tests.

use aipdf::{
    build_aipdf, extract_semantic_xml, semantic_xml_from_source, validate_xml, BuildOptions,
    RenderMode, SourceKind,
};

fn build(xml: &str, mode: RenderMode) -> Vec<u8> {
    build_aipdf(
        xml,
        &BuildOptions {
            render: mode,
            ..Default::default()
        },
    )
    .expect("build")
}

#[test]
fn minimal_round_trip_is_exact() {
    // In minimal mode the embedded payload is the exact (sanitised) input XML.
    for src in [
        ("# Title\n\nHello world.", SourceKind::Markdown),
        (
            "<h1>Doc</h1><p>Body text here.</p><ul><li>a</li><li>b</li></ul>",
            SourceKind::Html,
        ),
    ] {
        let xml = semantic_xml_from_source(src.0, src.1).expect("to xml");
        let pdf = build(&xml, RenderMode::Minimal);
        let extracted = extract_semantic_xml(&pdf).expect("extract");
        assert_eq!(extracted, xml, "minimal round-trip must be byte-exact");
    }
}

#[test]
fn full_round_trip_preserves_content_and_validates() {
    let xml = semantic_xml_from_source(
        "# Heading\n\nA paragraph of text.\n\n- item one\n- item two",
        SourceKind::Markdown,
    )
    .unwrap();
    let pdf = build(&xml, RenderMode::Full);
    let extracted = extract_semantic_xml(&pdf).unwrap();
    validate_xml(&extracted).expect("extracted full XML must validate");
    for needle in ["Heading", "A paragraph of text.", "item one", "item two"] {
        assert!(extracted.contains(needle), "lost content: {needle}");
    }
    // Full render annotates coordinates.
    assert!(extracted.contains("bbox="), "full render should add bboxes");
}

#[test]
fn survives_third_party_reassembly() {
    // A PDF tool that re-serialises the file (normalising xref/objects) must not
    // break semantic-layer detection — extraction falls back to a structural
    // lopdf lookup that does not depend on our exact byte layout.
    let xml = semantic_xml_from_source("# T\n\nReassembly survival check.", SourceKind::Markdown)
        .unwrap();
    let pdf = build(&xml, RenderMode::Full);

    let doc = lopdf::Document::load_mem(&pdf).expect("load");
    let mut resaved = Vec::new();
    {
        let mut doc = doc;
        doc.save_to(&mut resaved).expect("re-save");
    }
    assert_ne!(resaved, pdf, "re-save should change bytes");

    let extracted = extract_semantic_xml(&resaved).expect("extract after re-save");
    assert!(extracted.contains("Reassembly survival check."));
}

#[test]
fn empty_and_whitespace_inputs_do_not_panic() {
    // Degenerate sources should produce a valid (possibly placeholder) document
    // rather than panic.
    for src in ["", "   \n\n  ", "#", "```"] {
        let xml = semantic_xml_from_source(src, SourceKind::Markdown).unwrap();
        validate_xml(&xml).expect("degenerate markdown still yields valid XML");
        let pdf = build(&xml, RenderMode::Full);
        assert!(extract_semantic_xml(&pdf).is_ok());
    }
}
