//! HTML input side: parse an HTML document with `scraper` and walk its DOM,
//! emitting semantic blocks through an [`HtmlConverter`].

use super::html::HtmlConverter;
use scraper::{ElementRef, Html, Selector};

pub(crate) fn html_to_xml(input: &str) -> String {
    let document = Html::parse_document(input);
    let mut conv = HtmlConverter::new();

    // Extract <title> from <head> for the document
    let head_title = Selector::parse("head title").ok();
    let doc_title = head_title
        .as_ref()
        .and_then(|sel| document.select(sel).next())
        .map(|el| element_text(&el));

    // Walk <body> (or full document if no body)
    let body_sel = Selector::parse("body").ok();
    let root_iter: Box<dyn Iterator<Item = ElementRef<'_>>> = if let Some(sel) = &body_sel {
        if let Some(body) = document.select(sel).next() {
            Box::new(body.children().filter_map(ElementRef::wrap))
        } else {
            Box::new(
                document
                    .root_element()
                    .children()
                    .filter_map(ElementRef::wrap),
            )
        }
    } else {
        Box::new(
            document
                .root_element()
                .children()
                .filter_map(ElementRef::wrap),
        )
    };

    // If we found a head title and no headings precede body content, open a default section
    let _ = doc_title; // used if needed below

    walk_elements(root_iter, &mut conv);

    let blocks = conv.finish();
    if blocks.is_empty() {
        // Fallback: no content found, produce minimal document
        return super::wrap_document(vec![
            r#"<section id="s1" level="1" page="1">"#.to_string(),
            r#"<paragraph id="b1" page="1" role="paragraph">Document</paragraph>"#.to_string(),
            "</section>".to_string(),
        ]);
    }
    super::wrap_document(blocks)
}

pub(crate) fn walk_elements<'a>(
    elements: impl Iterator<Item = ElementRef<'a>>,
    conv: &mut HtmlConverter,
) {
    for el in elements {
        let tag = el.value().name().to_lowercase();
        match tag.as_str() {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = tag[1..].parse::<usize>().unwrap_or(1);
                let text = element_text(&el);
                if !text.trim().is_empty() {
                    conv.open_section(level, text.trim());
                }
            }
            "p" => {
                let text = element_text(&el);
                conv.push_paragraph(&text);
            }
            "ul" | "ol" => {
                let ordered = tag == "ol";
                let items = collect_list_items(&el);
                conv.push_list(&items, ordered);
            }
            "table" => {
                let (rows, caption) = collect_table(&el);
                conv.push_table(&rows, caption.as_deref());
            }
            "pre" => {
                let code_el = el
                    .children()
                    .filter_map(ElementRef::wrap)
                    .find(|c| c.value().name() == "code");
                let (text, lang) = if let Some(code) = code_el {
                    let lang = extract_code_language(&code);
                    (element_text(&code), lang)
                } else {
                    (element_text(&el), None)
                };
                conv.push_code_block(&text, lang.as_deref());
            }
            "blockquote" => {
                let text = element_text(&el);
                conv.push_citation(&text);
            }
            "figure" => {
                let img = el
                    .children()
                    .filter_map(ElementRef::wrap)
                    .find(|c| c.value().name() == "img");
                let (src, alt) = img
                    .map(|i| {
                        (
                            i.value().attr("src").unwrap_or("").to_string(),
                            i.value().attr("alt").unwrap_or("").to_string(),
                        )
                    })
                    .unwrap_or_default();
                let caption = el
                    .children()
                    .filter_map(ElementRef::wrap)
                    .find(|c| c.value().name() == "figcaption")
                    .map(|c| element_text(&c));
                conv.push_figure(&src, &alt, caption.as_deref());
            }
            // Skip layout/nav/script/style and recurse into semantic containers
            "div" | "main" | "article" | "section" | "aside" | "header" | "footer" | "nav"
            | "span" => {
                walk_elements(el.children().filter_map(ElementRef::wrap), conv);
            }
            "script" | "style" | "noscript" | "head" => {} // skip entirely
            _ => {
                // For unknown elements, recurse into children
                walk_elements(el.children().filter_map(ElementRef::wrap), conv);
            }
        }
    }
}

pub(crate) fn collect_list_items(el: &ElementRef<'_>) -> Vec<String> {
    let mut items = Vec::new();
    for child in el.children().filter_map(ElementRef::wrap) {
        if child.value().name() == "li" {
            items.push(element_text(&child));
        }
    }
    items
}

pub(crate) fn collect_table(el: &ElementRef<'_>) -> (Vec<Vec<(String, bool)>>, Option<String>) {
    let mut rows: Vec<Vec<(String, bool)>> = Vec::new();
    let mut caption: Option<String> = None;

    for child in el.children().filter_map(ElementRef::wrap) {
        match child.value().name() {
            "caption" => caption = Some(element_text(&child)),
            "thead" | "tbody" | "tfoot" => {
                collect_rows_from(&child, &mut rows);
            }
            "tr" => {
                let row = collect_cells_from_row(&child);
                if !row.is_empty() {
                    rows.push(row);
                }
            }
            _ => {}
        }
    }
    (rows, caption)
}

pub(crate) fn collect_rows_from(el: &ElementRef<'_>, rows: &mut Vec<Vec<(String, bool)>>) {
    for child in el.children().filter_map(ElementRef::wrap) {
        if child.value().name() == "tr" {
            let row = collect_cells_from_row(&child);
            if !row.is_empty() {
                rows.push(row);
            }
        }
    }
}

pub(crate) fn collect_cells_from_row(row: &ElementRef<'_>) -> Vec<(String, bool)> {
    let mut cells = Vec::new();
    for child in row.children().filter_map(ElementRef::wrap) {
        match child.value().name() {
            "th" => cells.push((element_text(&child), true)),
            "td" => cells.push((element_text(&child), false)),
            _ => {}
        }
    }
    cells
}

pub(crate) fn extract_code_language(el: &ElementRef<'_>) -> Option<String> {
    el.value().attr("class").and_then(|cls| {
        cls.split_whitespace()
            .find(|c| c.starts_with("language-"))
            .map(|c| c["language-".len()..].to_string())
    })
}

pub(crate) fn element_text(el: &ElementRef<'_>) -> String {
    el.text().collect::<Vec<_>>().join(" ")
}
