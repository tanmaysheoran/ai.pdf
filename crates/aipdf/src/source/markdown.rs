use super::html::{HtmlConverter, ListCtx, TableCtx, heading_level};
use super::wrap_document;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

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
