//! Verifies that `full` render writes real page/bbox coordinates back into the
//! embedded semantic XML (instead of the hardcoded page="1" / empty bbox).

use aipdf::{build_aipdf, extract_semantic_xml, get_reading_order, BuildOptions, RenderMode};

fn long_doc(n: usize) -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<document version=\"1.0\" id=\"doc1\" lang=\"en\">\n<section id=\"s1\" level=\"1\" page=\"1\">\n",
    );
    s.push_str("<title id=\"t1\" page=\"1\" role=\"title\">Coordinates</title>\n");
    for i in 0..n {
        s.push_str(&format!(
            "<paragraph id=\"p{i}\" page=\"1\" role=\"paragraph\">Paragraph number {i} with enough words to occupy a line of the page body text.</paragraph>\n"
        ));
    }
    s.push_str("</section>\n</document>");
    s
}

#[test]
fn full_render_writes_page_and_bbox() {
    let pdf = build_aipdf(
        &long_doc(60),
        &BuildOptions {
            render: RenderMode::Full,
            ..Default::default()
        },
    )
    .unwrap();

    let xml = extract_semantic_xml(&pdf).unwrap();
    let blocks = get_reading_order(&xml).unwrap();

    // Title sits on page 1 with a valid 4-number bbox inside the page.
    let title = blocks.iter().find(|b| b.id.as_deref() == Some("t1")).unwrap();
    assert_eq!(title.page, Some(1));
    let bbox = title.bbox.as_ref().expect("title bbox present");
    let nums: Vec<f32> = bbox.split(',').map(|s| s.parse().unwrap()).collect();
    assert_eq!(nums.len(), 4, "bbox is x0,y0,x1,y1: {bbox}");
    let (x0, y0, x1, y1) = (nums[0], nums[1], nums[2], nums[3]);
    assert!(x1 > x0 && y1 > y0, "bbox ordering: {bbox}");
    assert!(y1 <= 792.0 && y0 >= 0.0, "bbox within letter page: {bbox}");

    // Content long enough to spill onto later pages.
    let max_page = blocks.iter().filter_map(|b| b.page).max().unwrap();
    assert!(max_page >= 2, "expected multi-page layout, got {max_page}");

    // Not every paragraph is page 1 anymore (the old code hardcoded page=1).
    let on_page_2 = blocks.iter().filter(|b| b.page == Some(2)).count();
    assert!(on_page_2 > 0, "no blocks landed on page 2");
}
