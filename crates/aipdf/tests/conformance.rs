//! Conformance: the Rust transforms reproduce the committed golden fixtures.
//! The Python and TypeScript SDKs assert the *same* goldens for `onto`,
//! `markdown`, and `markdown-ast`, so all three implementations are pinned to
//! one source of truth. The `rich.ast.json` golden also guards the figure/image
//! regression where self-closing `<image/>` nodes were silently dropped.

use aipdf::{xml_to_markdown, xml_to_markdown_ast_json, xml_to_onto};

fn root() -> std::path::PathBuf {
    // crates/aipdf -> repo root
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn read(p: &str) -> String {
    std::fs::read_to_string(root().join(p)).unwrap_or_else(|e| panic!("read {p}: {e}"))
}

fn case(name: &str) -> String {
    match name {
        "minimal" => read("samples/minimal.xml"),
        other => read(&format!("tests/conformance/{other}.xml")),
    }
}

#[test]
fn onto_matches_golden() {
    for name in ["minimal", "rich"] {
        let xml = case(name);
        let golden = read(&format!("tests/conformance/{name}.onto"));
        assert_eq!(
            xml_to_onto(&xml).trim_end(),
            golden.trim_end(),
            "ONTO mismatch for {name}"
        );
    }
}

#[test]
fn markdown_matches_golden() {
    for name in ["minimal", "rich"] {
        let xml = case(name);
        let golden = read(&format!("tests/conformance/{name}.md"));
        assert_eq!(
            xml_to_markdown(&xml).trim_end(),
            golden.trim_end(),
            "Markdown mismatch for {name}"
        );
    }
}

#[test]
fn markdown_ast_matches_golden() {
    // `rich.xml` carries a `<figure>` with a self-closing `<image/>`, so this
    // golden pins the image node into the AST output (regression guard).
    let xml = case("rich");
    let golden = read("tests/conformance/rich.ast.json");
    let got = xml_to_markdown_ast_json(&xml);
    assert_eq!(got.trim_end(), golden.trim_end(), "Markdown-AST mismatch");
    // Belt-and-braces: the figure's image must survive into the AST.
    assert!(
        got.contains("\"type\": \"image\"") && got.contains("\"url\": \"chart.png\""),
        "AST dropped the figure image"
    );
}
