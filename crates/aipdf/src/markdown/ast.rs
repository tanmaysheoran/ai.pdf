use quick_xml::events::Event;
use quick_xml::Reader;

use super::{MarkdownAst, MarkdownNode, attr_value};
use super::ast_nodes::{
    blockquote_node, heading_node, image_paragraph_node, list_item_node, paragraph_node,
    table_node, value_node,
};

pub fn xml_to_markdown_ast_json(xml: &str) -> String {
    let ast = xml_to_markdown_ast(xml);
    serde_json::to_string_pretty(&ast)
        .unwrap_or_else(|_| "{\"type\":\"root\",\"children\":[]}".to_string())
}

fn xml_to_markdown_ast(xml: &str) -> MarkdownAst {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut children = Vec::new();
    let mut current = String::new();
    let mut section_level = 1usize;
    let mut code_lang: Option<String> = None;
    let mut definition_term = String::new();
    let mut capture: Option<&'static str> = None;
    let mut in_table = false;
    let mut in_row = false;
    let mut in_cell = false;
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut in_list = false;
    let mut list_items: Vec<MarkdownNode> = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"section" => {
                    section_level = attr_value(&e, b"level")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1);
                }
                b"title" => start_capture(&mut current, &mut capture, "heading"),
                b"paragraph" => start_capture(&mut current, &mut capture, "paragraph"),
                b"caption" => start_capture(&mut current, &mut capture, "caption"),
                b"citation" => start_capture(&mut current, &mut capture, "citation"),
                b"equation" => start_capture(&mut current, &mut capture, "math"),
                b"codeBlock" => {
                    code_lang = attr_value(&e, b"language");
                    start_capture(&mut current, &mut capture, "code");
                }
                b"note" => start_capture(&mut current, &mut capture, "note"),
                b"footnote" => start_capture(&mut current, &mut capture, "footnote"),
                b"reference" => start_capture(&mut current, &mut capture, "reference"),
                b"definition" => {
                    definition_term = attr_value(&e, b"term").unwrap_or_default();
                    start_capture(&mut current, &mut capture, "definition");
                }
                b"list" | b"references" | b"definitionList" => {
                    in_list = true;
                    list_items.clear();
                }
                b"item" if in_list => start_capture(&mut current, &mut capture, "item"),
                b"table" => {
                    in_table = true;
                    table_rows.clear();
                }
                b"row" if in_table => {
                    in_row = true;
                    current_row.clear();
                }
                b"cell" if in_row => {
                    in_cell = true;
                    current.clear();
                }
                _ => {}
            },
            Ok(Event::Text(t)) => {
                if capture.is_some() || in_cell {
                    current.push_str(t.unescape().unwrap_or_default().trim());
                }
            }
            Ok(Event::CData(t)) => {
                if capture.is_some() {
                    current.push_str(String::from_utf8_lossy(&t).trim());
                }
            }
            Ok(Event::Empty(e)) => {
                // `<image src=… alt=…/>` is self-closing, so it arrives as Empty.
                // Emit it as an MDAST image node (wrapped in a paragraph). The
                // figure's `<caption>` is still captured separately as its own
                // paragraph by the End handler below.
                if e.name().as_ref() == b"image" {
                    let src = attr_value(&e, b"src").unwrap_or_default();
                    let alt = attr_value(&e, b"alt").unwrap_or_default();
                    children.push(image_paragraph_node(&src, &alt));
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"title" if capture == Some("heading") => {
                    children.push(heading_node(section_level, current.trim()));
                    capture = None;
                }
                b"paragraph" if capture == Some("paragraph") => {
                    children.push(paragraph_node(current.trim()));
                    capture = None;
                }
                b"caption" if capture == Some("caption") => {
                    children.push(paragraph_node(current.trim()));
                    capture = None;
                }
                b"citation" if capture == Some("citation") => {
                    children.push(blockquote_node(current.trim()));
                    capture = None;
                }
                b"equation" if capture == Some("math") => {
                    children.push(value_node("math", current.trim(), None));
                    capture = None;
                }
                b"codeBlock" if capture == Some("code") => {
                    children.push(value_node("code", current.trim(), code_lang.take()));
                    capture = None;
                }
                b"note" if capture == Some("note") => {
                    children.push(blockquote_node(&format!("Note: {}", current.trim())));
                    capture = None;
                }
                b"footnote" if capture == Some("footnote") => {
                    children.push(MarkdownNode {
                        node_type: "footnoteDefinition",
                        url: None,
                        alt: None,
                        value: None,
                        depth: None,
                        lang: None,
                        ordered: None,
                        children: vec![paragraph_node(current.trim())],
                    });
                    capture = None;
                }
                b"item" if capture == Some("item") => {
                    list_items.push(list_item_node(current.trim()));
                    capture = None;
                }
                b"reference" if capture == Some("reference") => {
                    list_items.push(list_item_node(current.trim()));
                    capture = None;
                }
                b"definition" if capture == Some("definition") => {
                    let value = if definition_term.is_empty() {
                        current.trim().to_string()
                    } else {
                        format!("{}: {}", definition_term, current.trim())
                    };
                    list_items.push(list_item_node(&value));
                    capture = None;
                }
                b"list" | b"references" | b"definitionList" if in_list => {
                    children.push(MarkdownNode {
                        node_type: "list",
                        url: None,
                        alt: None,
                        value: None,
                        depth: None,
                        lang: None,
                        ordered: Some(false),
                        children: list_items.clone(),
                    });
                    in_list = false;
                }
                b"cell" if in_cell => {
                    current_row.push(current.trim().to_string());
                    in_cell = false;
                }
                b"row" if in_row => {
                    table_rows.push(current_row.clone());
                    in_row = false;
                }
                b"table" if in_table => {
                    children.push(table_node(&table_rows));
                    in_table = false;
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            _ => {}
        }
    }

    MarkdownAst {
        node_type: "root",
        children,
    }
}

pub(super) fn start_capture(current: &mut String, capture: &mut Option<&'static str>, kind: &'static str) {
    current.clear();
    *capture = Some(kind);
}
