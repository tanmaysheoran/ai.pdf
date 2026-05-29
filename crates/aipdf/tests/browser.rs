//! Browser render path: render HTML (CSS and all) to a PDF with headless Chrome
//! and attach the semantic layer. Skipped automatically when no Chrome/Chromium
//! binary is available, so it is a no-op on machines without a browser.

use aipdf::{
    build_aipdf_browser, chrome_available, extract_semantic_xml, inspect_pdf, BuildOptions,
};

#[test]
fn browser_render_attaches_detectable_layer() {
    if !chrome_available() {
        eprintln!("skipping: no Chrome/Chromium found");
        return;
    }

    let html = r#"<!DOCTYPE html>
<html><head><title>Browser Test</title>
<style>h1 { color: #0969da; } code { background: #161b22; color: #fff; }</style>
</head><body>
<h1>Browser Render Heading</h1>
<p>A styled paragraph with <code>inline code</code>.</p>
<ul><li>first item</li><li>second item</li></ul>
</body></html>"#;

    // Write into the temp dir so no `.aipdf-print-*` file lands in the repo.
    let base = std::env::temp_dir();
    let out = build_aipdf_browser(html, Some(&base), &BuildOptions::default())
        .expect("browser render should succeed when Chrome is present");

    assert!(out.starts_with(b"%PDF-"), "output must be a PDF");

    let report = inspect_pdf(&out);
    assert!(
        report.has_semantic_layer,
        "browser-rendered file must carry a detectable semantic layer"
    );

    let xml = extract_semantic_xml(&out).expect("semantic layer must be extractable");
    assert!(xml.contains("Browser Render Heading"), "heading text: {xml}");
    assert!(xml.contains("inline code"), "inline code text: {xml}");
    assert!(xml.contains("second item"), "list item text: {xml}");

    // The browser source format is recorded in the XMP metadata stream (not the
    // semantic payload); it should be present in the raw PDF bytes.
    assert!(
        String::from_utf8_lossy(&out).contains("html-browser"),
        "XMP should record the browser source format"
    );
}
