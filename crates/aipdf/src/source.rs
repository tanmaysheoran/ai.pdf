use crate::{security::sanitize_xml, xml::validate_xml, AipdfError, Result};
use scraper::{ElementRef, Html, Selector};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Xml,
    Markdown,
    Html,
    Typst,
}

impl SourceKind {
    pub fn from_path(path: &Path) -> Result<Self> {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
        {
            "xml" => Ok(Self::Xml),
            "md" | "markdown" => Ok(Self::Markdown),
            "html" | "htm" => Ok(Self::Html),
            "typ" | "typst" => Ok(Self::Typst),
            other => Err(AipdfError::InvalidXml(format!(
                "unsupported input extension `{other}`"
            ))),
        }
    }
}

pub fn semantic_xml_from_source(input: &str, kind: SourceKind) -> Result<String> {
    let xml = match kind {
        SourceKind::Xml => sanitize_xml(&extract_xml_payload(input))?,
        SourceKind::Markdown => markdown_to_xml(input),
        SourceKind::Html => html_to_xml(input),
        SourceKind::Typst => typst_to_xml(input),
    };
    validate_xml(&xml)?;
    Ok(xml)
}

fn extract_xml_payload(input: &str) -> String {
    let raw = if let Some(start) = input.find("```xml") {
        let after_start = &input[start + "```xml".len()..];
        if let Some(end) = after_start.find("```") {
            after_start[..end].trim().to_string()
        } else {
            input.trim().to_string()
        }
    } else {
        input.trim().to_string()
    };
    // Strip xml-stylesheet processing instructions: they are browser-preview hints only
    // and must not be embedded in the .aipdf semantic payload (validate_xml rejects PIs).
    raw.lines()
        .filter(|l| !l.trim().starts_with("<?xml-stylesheet"))
        .collect::<Vec<_>>()
        .join("\n")
}

// ── HTML → semantic XML ──────────────────────────────────────────────────────

struct HtmlConverter {
    blocks: Vec<String>,
    section_id: usize,
    block_id: usize,
    section_open: bool,
}

impl HtmlConverter {
    fn new() -> Self {
        Self {
            blocks: Vec::new(),
            section_id: 1,
            block_id: 1,
            section_open: false,
        }
    }

    fn open_section(&mut self, level: usize, title: &str) {
        if self.section_open {
            self.blocks.push("</section>".to_string());
        }
        let sid = self.section_id;
        self.section_id += 1;
        let bid = self.block_id;
        self.block_id += 1;
        self.blocks.push(format!(
            r#"<section id="s{sid}" level="{level}" page="1">"#
        ));
        self.blocks.push(format!(
            r#"<title id="b{bid}" page="1" role="title">{}</title>"#,
            xml_escape(title)
        ));
        self.section_open = true;
    }

    fn ensure_section(&mut self) {
        if !self.section_open {
            let sid = self.section_id;
            self.section_id += 1;
            self.blocks
                .push(format!(r#"<section id="s{sid}" level="1" page="1">"#));
            self.section_open = true;
        }
    }

    fn push_paragraph(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.ensure_section();
        let bid = self.block_id;
        self.block_id += 1;
        self.blocks.push(format!(
            r#"<paragraph id="b{bid}" page="1" role="paragraph">{}</paragraph>"#,
            xml_escape(text)
        ));
    }

    fn push_list(&mut self, items: &[String], ordered: bool) {
        if items.is_empty() {
            return;
        }
        self.ensure_section();
        let bid = self.block_id;
        self.block_id += 1;
        let list_type = if ordered { "ordered" } else { "unordered" };
        let mut list = format!(r#"<list id="b{bid}" type="{list_type}">"#);
        for item in items {
            let ibid = self.block_id;
            self.block_id += 1;
            list.push_str(&format!(
                r#"<item id="b{ibid}">{}</item>"#,
                xml_escape(item.trim())
            ));
        }
        list.push_str("</list>");
        self.blocks.push(list);
    }

    fn push_table(&mut self, rows: &[Vec<(String, bool)>], caption: Option<&str>) {
        if rows.is_empty() {
            return;
        }
        self.ensure_section();
        let bid = self.block_id;
        self.block_id += 1;
        let mut tbl = format!(r#"<table id="b{bid}" page="1" role="table">"#);
        if let Some(cap) = caption {
            let cbid = self.block_id;
            self.block_id += 1;
            tbl.push_str(&format!(
                r#"<caption id="b{cbid}" role="caption">{}</caption>"#,
                xml_escape(cap.trim())
            ));
        }
        for row in rows {
            tbl.push_str("<row>");
            for (cell_text, is_header) in row {
                let header_attr = if *is_header { r#" header="true""# } else { "" };
                tbl.push_str(&format!(
                    "<cell{header_attr}>{}</cell>",
                    xml_escape(cell_text.trim())
                ));
            }
            tbl.push_str("</row>");
        }
        tbl.push_str("</table>");
        self.blocks.push(tbl);
    }

    fn push_code_block(&mut self, text: &str, language: Option<&str>) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.ensure_section();
        let bid = self.block_id;
        self.block_id += 1;
        let lang_attr = language
            .map(|l| format!(r#" language="{}""#, xml_escape(l)))
            .unwrap_or_default();
        self.blocks.push(format!(
            r#"<codeBlock id="b{bid}"{lang_attr}>{}</codeBlock>"#,
            xml_escape(text)
        ));
    }

    fn push_citation(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.ensure_section();
        let bid = self.block_id;
        self.block_id += 1;
        self.blocks.push(format!(
            r#"<citation id="b{bid}" role="citation">{}</citation>"#,
            xml_escape(text)
        ));
    }

    fn push_figure(&mut self, src: &str, alt: &str, caption: Option<&str>) {
        self.ensure_section();
        let bid = self.block_id;
        self.block_id += 1;
        let mut fig = format!(r#"<figure id="b{bid}" page="1">"#);
        if !src.is_empty() || !alt.is_empty() {
            fig.push_str(&format!(
                r#"<image src="{}" alt="{}"/>"#,
                xml_escape(src),
                xml_escape(alt)
            ));
        }
        if let Some(cap) = caption {
            let cbid = self.block_id;
            self.block_id += 1;
            fig.push_str(&format!(
                r#"<caption id="b{cbid}">{}</caption>"#,
                xml_escape(cap.trim())
            ));
        }
        fig.push_str("</figure>");
        self.blocks.push(fig);
    }

    fn finish(mut self) -> Vec<String> {
        if self.section_open {
            self.blocks.push("</section>".to_string());
        }
        self.blocks
    }
}

fn html_to_xml(input: &str) -> String {
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
        return wrap_document(vec![
            r#"<section id="s1" level="1" page="1">"#.to_string(),
            r#"<paragraph id="b1" page="1" role="paragraph">Document</paragraph>"#.to_string(),
            "</section>".to_string(),
        ]);
    }
    wrap_document(blocks)
}

fn walk_elements<'a>(
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

fn collect_list_items(el: &ElementRef<'_>) -> Vec<String> {
    let mut items = Vec::new();
    for child in el.children().filter_map(ElementRef::wrap) {
        if child.value().name() == "li" {
            items.push(element_text(&child));
        }
    }
    items
}

fn collect_table(el: &ElementRef<'_>) -> (Vec<Vec<(String, bool)>>, Option<String>) {
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

fn collect_rows_from(el: &ElementRef<'_>, rows: &mut Vec<Vec<(String, bool)>>) {
    for child in el.children().filter_map(ElementRef::wrap) {
        if child.value().name() == "tr" {
            let row = collect_cells_from_row(&child);
            if !row.is_empty() {
                rows.push(row);
            }
        }
    }
}

fn collect_cells_from_row(row: &ElementRef<'_>) -> Vec<(String, bool)> {
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

fn extract_code_language(el: &ElementRef<'_>) -> Option<String> {
    el.value().attr("class").and_then(|cls| {
        cls.split_whitespace()
            .find(|c| c.starts_with("language-"))
            .map(|c| c["language-".len()..].to_string())
    })
}

fn element_text(el: &ElementRef<'_>) -> String {
    el.text().collect::<Vec<_>>().join(" ")
}

// ── Markdown → semantic XML ──────────────────────────────────────────────────

pub(crate) fn markdown_to_xml(input: &str) -> String {
    let mut blocks = Vec::new();
    let mut section_id = 1usize;
    let mut block_id = 1usize;
    let mut open = false;

    for paragraph in paragraphs(input) {
        if let Some((level, title)) = markdown_heading(&paragraph) {
            if open {
                blocks.push("</section>".to_string());
            }
            blocks.push(format!(
                r#"<section id="s{section_id}" level="{level}" page="1">"#
            ));
            blocks.push(format!(
                r#"<title id="b{block_id}" page="1" role="title">{}</title>"#,
                xml_escape(title)
            ));
            section_id += 1;
            block_id += 1;
            open = true;
        } else {
            if !open {
                blocks.push(r#"<section id="s1" level="1" page="1">"#.to_string());
                open = true;
                section_id = 2;
            }
            blocks.push(format!(
                r#"<paragraph id="b{block_id}" page="1" role="paragraph">{}</paragraph>"#,
                xml_escape(&paragraph)
            ));
            block_id += 1;
        }
    }
    if open {
        blocks.push("</section>".to_string());
    }
    wrap_document(blocks)
}

fn typst_to_xml(input: &str) -> String {
    let mut markdownish = String::new();
    for line in input.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("= ") {
            markdownish.push_str("# ");
            markdownish.push_str(title);
        } else if let Some(title) = trimmed.strip_prefix("== ") {
            markdownish.push_str("## ");
            markdownish.push_str(title);
        } else {
            markdownish.push_str(trimmed);
        }
        markdownish.push('\n');
    }
    markdown_to_xml(&markdownish)
}

fn paragraphs(input: &str) -> Vec<String> {
    input
        .split("\n\n")
        .map(|p| p.lines().map(str::trim).collect::<Vec<_>>().join(" "))
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn markdown_heading(input: &str) -> Option<(usize, &str)> {
    let hashes = input.chars().take_while(|c| *c == '#').count();
    if hashes > 0 && hashes <= 6 && input.as_bytes().get(hashes) == Some(&b' ') {
        Some((hashes, input[hashes + 1..].trim()))
    } else {
        None
    }
}

fn wrap_document(blocks: Vec<String>) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<document version=\"1.0\" id=\"doc1\" lang=\"en\">\n{}\n</document>",
        blocks.join("\n")
    )
}

pub(crate) fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
