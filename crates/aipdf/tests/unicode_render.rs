//! End-to-end check that non-ASCII text survives into the *visible* PDF layer
//! (not just the semantic XML), via the embedded CID font + ToUnicode CMap.

use aipdf::{build_aipdf, extract_semantic_xml, BuildOptions, RenderMode};

const UNICODE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<document version="1.0" id="doc1" lang="en">
  <section id="s1" level="1" page="1">
    <title id="b1" role="title">Café Ωμέγα — Жираф</title>
    <paragraph id="b2" role="paragraph">Voilà: naïve résumé, piñata, Москва, Ελλάδα.</paragraph>
  </section>
</document>"#;

fn rendered_text(mode: RenderMode) -> String {
    let pdf = build_aipdf(
        UNICODE_XML,
        &BuildOptions {
            title: "Unicode".into(),
            render: mode,
            ..Default::default()
        },
    )
    .expect("build");

    // Round-trip the semantic layer first.
    let xml = extract_semantic_xml(&pdf).expect("extract semantic xml");
    assert!(xml.contains("Жираф"), "semantic XML must preserve Unicode");

    // Now decode the *visible* text using lopdf (which honours ToUnicode).
    let doc = lopdf::Document::load_mem(&pdf).expect("lopdf parse");
    let pages: Vec<u32> = doc.get_pages().keys().copied().collect();
    doc.extract_text(&pages).expect("extract visible text")
}

#[test]
fn unicode_survives_full_render() {
    let text = rendered_text(RenderMode::Full);
    for needle in ["Café", "Ωμέγα", "Жираф", "naïve", "résumé", "Москва", "Ελλάδα"] {
        assert!(
            text.contains(needle),
            "visible (full) text missing {needle:?}; got: {text:?}"
        );
    }
}

#[test]
fn unicode_survives_minimal_render() {
    let text = rendered_text(RenderMode::Minimal);
    for needle in ["Café", "Жираф", "piñata"] {
        assert!(
            text.contains(needle),
            "visible (minimal) text missing {needle:?}; got: {text:?}"
        );
    }
}
