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

    fn push_equation(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.ensure_section();
        let bid = self.block_id;
        self.block_id += 1;
        self.blocks.push(format!(
            r#"<equation id="b{bid}">{}</equation>"#,
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

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

fn heading_level(h: HeadingLevel) -> usize {
    match h {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// A pending list being built (lists can nest; each level is flushed when its
/// `List` end tag arrives).
struct ListCtx {
    ordered: bool,
    items: Vec<String>,
}

/// A pending table being built.
struct TableCtx {
    rows: Vec<Vec<(String, bool)>>,
    cur: Vec<(String, bool)>,
    in_header: bool,
}

pub(crate) fn markdown_to_xml(input: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(input, opts);

    let mut conv = HtmlConverter::new();
    // Inline text accumulator for the block currently being assembled.
    let mut inline = String::new();
    let mut heading: Option<usize> = None;
    let mut lists: Vec<ListCtx> = Vec::new();
    let mut in_item = false;
    let mut code: Option<Option<String>> = None; // Some(lang?) while inside a code block
    let mut quote: Option<String> = None; // accumulates blockquote text -> citation
    let mut table: Option<TableCtx> = None;
    let mut image: Option<(String, String)> = None; // (src, alt accumulator)

    let push_text = |inline: &mut String,
                     quote: &mut Option<String>,
                     image: &mut Option<(String, String)>,
                     s: &str| {
        if let Some((_, alt)) = image.as_mut() {
            alt.push_str(s);
        } else if let Some(q) = quote.as_mut() {
            q.push_str(s);
        } else {
            inline.push_str(s);
        }
    };

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some(heading_level(level));
                inline.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(level) = heading.take() {
                    conv.open_section(level, inline.trim());
                }
                inline.clear();
            }
            Event::Start(Tag::Paragraph) => {
                if quote.is_none() && !in_item {
                    inline.clear();
                }
            }
            Event::End(TagEnd::Paragraph) => {
                // Inside a list item or blockquote the text is flushed by the
                // enclosing container; a top-level paragraph is flushed here.
                if quote.is_none() && !in_item {
                    conv.push_paragraph(inline.trim());
                    inline.clear();
                } else if quote.is_some() {
                    if let Some(q) = quote.as_mut() {
                        q.push(' ');
                    }
                }
            }
            Event::Start(Tag::List(start)) => {
                lists.push(ListCtx {
                    ordered: start.is_some(),
                    items: Vec::new(),
                });
            }
            Event::End(TagEnd::List(_)) => {
                if let Some(ctx) = lists.pop() {
                    conv.push_list(&ctx.items, ctx.ordered);
                }
            }
            Event::Start(Tag::Item) => {
                in_item = true;
                inline.clear();
            }
            Event::End(TagEnd::Item) => {
                in_item = false;
                if let Some(ctx) = lists.last_mut() {
                    ctx.items.push(inline.trim().to_string());
                }
                inline.clear();
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(l) if !l.is_empty() => Some(l.to_string()),
                    _ => None,
                };
                code = Some(lang);
                inline.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(lang) = code.take() {
                    conv.push_code_block(inline.trim_end_matches('\n'), lang.as_deref());
                }
                inline.clear();
            }
            Event::Start(Tag::BlockQuote(_)) => {
                quote = Some(String::new());
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                if let Some(q) = quote.take() {
                    conv.push_citation(q.trim());
                }
            }
            Event::Start(Tag::Table(_)) => {
                table = Some(TableCtx {
                    rows: Vec::new(),
                    cur: Vec::new(),
                    in_header: false,
                });
            }
            Event::End(TagEnd::Table) => {
                if let Some(t) = table.take() {
                    conv.push_table(&t.rows, None);
                }
            }
            Event::Start(Tag::TableHead) => {
                if let Some(t) = table.as_mut() {
                    t.in_header = true;
                    t.cur.clear();
                }
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(t) = table.as_mut() {
                    let row = std::mem::take(&mut t.cur);
                    if !row.is_empty() {
                        t.rows.push(row);
                    }
                    t.in_header = false;
                }
            }
            Event::Start(Tag::TableRow) => {
                if let Some(t) = table.as_mut() {
                    t.cur.clear();
                }
            }
            Event::End(TagEnd::TableRow) => {
                if let Some(t) = table.as_mut() {
                    let row = std::mem::take(&mut t.cur);
                    if !row.is_empty() {
                        t.rows.push(row);
                    }
                }
            }
            Event::Start(Tag::TableCell) => {
                inline.clear();
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(t) = table.as_mut() {
                    let is_header = t.in_header;
                    t.cur.push((inline.trim().to_string(), is_header));
                }
                inline.clear();
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                image = Some((dest_url.to_string(), String::new()));
            }
            Event::End(TagEnd::Image) => {
                if let Some((src, alt)) = image.take() {
                    conv.push_figure(&src, alt.trim(), None);
                }
            }
            // Inline emphasis/links are flattened to their text content.
            Event::Text(s) => push_text(&mut inline, &mut quote, &mut image, &s),
            Event::Code(s) => push_text(&mut inline, &mut quote, &mut image, &s),
            Event::SoftBreak | Event::HardBreak => {
                push_text(&mut inline, &mut quote, &mut image, " ")
            }
            _ => {}
        }
    }

    let blocks = conv.finish();
    if blocks.is_empty() {
        return wrap_document(vec![
            r#"<section id="s1" level="1" page="1">"#.to_string(),
            r#"<paragraph id="b1" page="1" role="paragraph">Document</paragraph>"#.to_string(),
            "</section>".to_string(),
        ]);
    }
    wrap_document(blocks)
}

// ── Typst → semantic XML ──────────────────────────────────────────────────────
//
// A pragmatic line-based converter. It covers the common Typst block
// constructs; full Typst (scripting, `#let`, templates, content functions,
// math layout) is out of scope and unsupported markup is flattened to text.

fn typst_to_xml(input: &str) -> String {
    let mut conv = HtmlConverter::new();
    let mut para: Vec<String> = Vec::new();
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let t = line.trim();

        if t.is_empty() {
            flush_typst_para(&mut conv, &mut para);
            i += 1;
            continue;
        }

        // Fenced code: ```lang ... ```
        if let Some(rest) = t.strip_prefix("```") {
            flush_typst_para(&mut conv, &mut para);
            let lang = rest.trim();
            let lang = (!lang.is_empty()).then(|| lang.to_string());
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code.push(lines[i]);
                i += 1;
            }
            i += 1; // skip closing fence
            conv.push_code_block(&code.join("\n"), lang.as_deref());
            continue;
        }

        // Headings: one or more '=' then a space.
        if let Some((level, title)) = typst_heading(t) {
            flush_typst_para(&mut conv, &mut para);
            conv.open_section(level, &strip_typst_inline(title));
            i += 1;
            continue;
        }

        // Figure / image: `image("path")`, optionally inside `#figure(...)`.
        if let Some(src) = typst_image_src(t) {
            flush_typst_para(&mut conv, &mut para);
            let caption = typst_caption(t);
            conv.push_figure(&src, "", caption.as_deref());
            i += 1;
            continue;
        }

        // Block equation: a line beginning with `$`.
        if t.starts_with('$') {
            flush_typst_para(&mut conv, &mut para);
            let mut buf = vec![t.to_string()];
            if !(t.len() > 1 && t.ends_with('$')) {
                i += 1;
                while i < lines.len() {
                    let lt = lines[i].trim();
                    buf.push(lt.to_string());
                    if lt.ends_with('$') {
                        break;
                    }
                    i += 1;
                }
            }
            i += 1;
            let joined = buf.join(" ");
            let inner = joined.trim().trim_matches('$').trim();
            conv.push_equation(inner);
            continue;
        }

        // Lists: consecutive `- ` (unordered) or `+ ` (ordered) items.
        if is_typst_list_item(t) {
            flush_typst_para(&mut conv, &mut para);
            let ordered = t.starts_with("+ ");
            let mut items = Vec::new();
            while i < lines.len() {
                let lt = lines[i].trim();
                if is_typst_list_item(lt) {
                    items.push(strip_typst_inline(lt[2..].trim()));
                    i += 1;
                } else {
                    break;
                }
            }
            conv.push_list(&items, ordered);
            continue;
        }

        para.push(t.to_string());
        i += 1;
    }
    flush_typst_para(&mut conv, &mut para);

    let blocks = conv.finish();
    if blocks.is_empty() {
        return wrap_document(vec![
            r#"<section id="s1" level="1" page="1">"#.to_string(),
            r#"<paragraph id="b1" page="1" role="paragraph">Document</paragraph>"#.to_string(),
            "</section>".to_string(),
        ]);
    }
    wrap_document(blocks)
}

fn flush_typst_para(conv: &mut HtmlConverter, para: &mut Vec<String>) {
    if !para.is_empty() {
        let text = para.join(" ");
        conv.push_paragraph(&strip_typst_inline(&text));
        para.clear();
    }
}

fn typst_heading(t: &str) -> Option<(usize, &str)> {
    let eqs = t.chars().take_while(|c| *c == '=').count();
    if eqs >= 1 && t.as_bytes().get(eqs) == Some(&b' ') {
        Some((eqs.min(6), t[eqs + 1..].trim()))
    } else {
        None
    }
}

fn is_typst_list_item(t: &str) -> bool {
    t.starts_with("- ") || t.starts_with("+ ")
}

/// Extract the first `image("...")` source path from a line, if present.
fn typst_image_src(t: &str) -> Option<String> {
    let start = t.find("image(")? + "image(".len();
    let rest = &t[start..];
    let q1 = rest.find('"')? + 1;
    let q2 = rest[q1..].find('"')? + q1;
    Some(rest[q1..q2].to_string())
}

/// Extract a `caption: [..]` value from a `#figure(...)` line, if present.
fn typst_caption(t: &str) -> Option<String> {
    let start = t.find("caption:")? + "caption:".len();
    let rest = t[start..].trim_start();
    let open = rest.find('[')? + 1;
    let close = rest[open..].find(']')? + open;
    Some(strip_typst_inline(rest[open..close].trim()))
}

/// Flatten light Typst inline markup. Emphasis markers `*` and inline-code
/// backticks are removed; other content (including `_`, which is common inside
/// identifiers) is preserved.
fn strip_typst_inline(s: &str) -> String {
    s.chars().filter(|c| *c != '*' && *c != '`').collect()
}

fn wrap_document(blocks: Vec<String>) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<document version=\"1.0\" id=\"doc1\" lang=\"en\">\n{}\n</document>",
        blocks.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_extracts_rich_structure() {
        let md = "# Title\n\nIntro para with **bold** and `code`.\n\n\
- one\n- two\n\n\
1. first\n2. second\n\n\
| A | B |\n|---|---|\n| 1 | 2 |\n\n\
```rust\nfn main() {}\n```\n\n\
> a quotation\n\n\
![alt text](img.png)\n";
        let xml = markdown_to_xml(md);
        assert!(xml.contains(r#"<section id="s1" level="1"#));
        assert!(xml.contains("role=\"title\">Title<"));
        assert!(xml.contains("<paragraph"));
        assert!(xml.contains(r#"<list id="b"#) && xml.contains(r#"type="unordered""#));
        assert!(xml.contains(r#"type="ordered""#), "ordered list: {xml}");
        assert!(xml.contains("<table") && xml.contains("<cell header=\"true\">A<"));
        assert!(xml.contains(r#"<codeBlock id="b"# ) && xml.contains(r#"language="rust""#));
        assert!(xml.contains("fn main() {}"));
        assert!(xml.contains("<citation") && xml.contains("a quotation"));
        assert!(xml.contains("<figure") && xml.contains(r#"src="img.png""#));
        assert!(xml.contains(r#"alt="alt text""#));
        // emphasis is flattened, not literal markers
        assert!(xml.contains("bold and code"));
        crate::xml::validate_xml(&xml).expect("generated markdown XML must validate");
    }

    #[test]
    fn typst_extracts_rich_structure() {
        let typ = "= Title\n\n\
Intro with *bold* text.\n\n\
== Subsection\n\n\
- alpha\n- beta\n\n\
+ first\n+ second\n\n\
```python\nprint(1)\n```\n\n\
$ E = m c^2 $\n\n\
#figure(image(\"chart.png\"), caption: [A *chart*])\n";
        let xml = typst_to_xml(typ);
        assert!(xml.contains("role=\"title\">Title<"));
        assert!(xml.contains(r#"level="2""#), "subsection level: {xml}");
        assert!(xml.contains(r#"type="unordered""#) && xml.contains(">alpha<"));
        assert!(xml.contains(r#"type="ordered""#) && xml.contains(">first<"));
        assert!(xml.contains(r#"language="python""#) && xml.contains("print(1)"));
        assert!(xml.contains("<equation") && xml.contains("E = m c^2"));
        assert!(xml.contains("<figure") && xml.contains(r#"src="chart.png""#));
        assert!(xml.contains("A chart"), "caption flattened: {xml}");
        // bold markers stripped
        assert!(xml.contains("bold text") && !xml.contains("*bold*"));
        crate::xml::validate_xml(&xml).expect("generated typst XML must validate");
    }
}

pub(crate) fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
