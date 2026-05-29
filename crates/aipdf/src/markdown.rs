use quick_xml::events::Event;
use quick_xml::Reader;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct MarkdownAst {
    #[serde(rename = "type")]
    node_type: &'static str,
    children: Vec<MarkdownNode>,
}

#[derive(Debug, Serialize, Clone)]
struct MarkdownNode {
    #[serde(rename = "type")]
    node_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    depth: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lang: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ordered: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    children: Vec<MarkdownNode>,
}

pub fn xml_to_markdown(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = String::new();
    let mut current = String::new();
    let mut in_title = false;
    let mut section_level = 1usize;
    let mut in_paragraph = false;
    let mut in_citation = false;
    let mut in_equation = false;
    let mut in_caption = false;
    let mut in_code = false;
    let mut in_item = false;
    let mut in_note = false;
    let mut in_footnote = false;
    let mut in_reference = false;
    let mut in_definition = false;
    let mut definition_term = String::new();
    let mut in_table = false;
    let mut in_row = false;
    let mut in_cell = false;
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut list_ordered = false;
    let mut list_index = 0usize;
    let mut code_lang: Option<String> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"section" => {
                    section_level = e
                        .attributes()
                        .flatten()
                        .find(|a| a.key.as_ref() == b"level")
                        .and_then(|a| String::from_utf8_lossy(a.value.as_ref()).parse().ok())
                        .unwrap_or(1);
                }
                b"title" => {
                    current.clear();
                    in_title = true;
                }
                b"paragraph" => {
                    current.clear();
                    in_paragraph = true;
                }
                b"caption" => {
                    current.clear();
                    in_caption = true;
                }
                b"citation" => {
                    current.clear();
                    in_citation = true;
                }
                b"equation" => {
                    current.clear();
                    in_equation = true;
                }
                b"codeBlock" => {
                    current.clear();
                    code_lang = e
                        .attributes()
                        .flatten()
                        .find(|a| a.key.as_ref() == b"language")
                        .map(|a| String::from_utf8_lossy(a.value.as_ref()).to_string());
                    in_code = true;
                }
                b"list" => {
                    list_ordered = e
                        .attributes()
                        .flatten()
                        .find(|a| a.key.as_ref() == b"type")
                        .map(|a| a.value.as_ref() == b"ordered")
                        .unwrap_or(false);
                    list_index = 0;
                }
                b"item" => {
                    current.clear();
                    in_item = true;
                }
                b"note" => {
                    current.clear();
                    in_note = true;
                }
                b"footnote" => {
                    current.clear();
                    in_footnote = true;
                }
                b"reference" => {
                    current.clear();
                    in_reference = true;
                }
                b"definition" => {
                    current.clear();
                    definition_term = e
                        .attributes()
                        .flatten()
                        .find(|a| a.key.as_ref() == b"term")
                        .map(|a| String::from_utf8_lossy(a.value.as_ref()).to_string())
                        .unwrap_or_default();
                    in_definition = true;
                }
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
                let text = t.unescape().unwrap_or_default();
                if in_title
                    || in_paragraph
                    || in_caption
                    || in_citation
                    || in_equation
                    || in_code
                    || in_item
                    || in_note
                    || in_footnote
                    || in_reference
                    || in_definition
                    || in_cell
                {
                    current.push_str(text.trim());
                }
            }
            Ok(Event::CData(t)) => {
                if in_equation || in_code {
                    current.push_str(String::from_utf8_lossy(&t).trim());
                }
            }
            Ok(Event::Empty(e)) => {
                if e.name().as_ref() == b"image" {
                    let attr = |k: &[u8]| {
                        e.attributes()
                            .flatten()
                            .find(|a| a.key.as_ref() == k)
                            .map(|a| String::from_utf8_lossy(a.value.as_ref()).to_string())
                            .unwrap_or_default()
                    };
                    out.push_str(&format!("![{}]({})\n\n", attr(b"alt"), attr(b"src")));
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"title" if in_title => {
                    out.push_str(&format!(
                        "{} {}\n\n",
                        "#".repeat(section_level.clamp(1, 6)),
                        current.trim()
                    ));
                    in_title = false;
                }
                b"paragraph" if in_paragraph => {
                    out.push_str(current.trim());
                    out.push_str("\n\n");
                    in_paragraph = false;
                }
                b"caption" if in_caption => {
                    out.push_str("_");
                    out.push_str(current.trim());
                    out.push_str("_\n\n");
                    in_caption = false;
                }
                b"citation" if in_citation => {
                    out.push_str("> ");
                    out.push_str(current.trim());
                    out.push_str("\n\n");
                    in_citation = false;
                }
                b"equation" if in_equation => {
                    out.push_str("```math\n");
                    out.push_str(current.trim());
                    out.push_str("\n```\n\n");
                    in_equation = false;
                }
                b"codeBlock" if in_code => {
                    out.push_str("```");
                    if let Some(lang) = code_lang.take() {
                        out.push_str(&lang);
                    }
                    out.push('\n');
                    out.push_str(current.trim());
                    out.push_str("\n```\n\n");
                    in_code = false;
                }
                b"item" if in_item => {
                    if list_ordered {
                        list_index += 1;
                        out.push_str(&format!("{list_index}. "));
                    } else {
                        out.push_str("- ");
                    }
                    out.push_str(current.trim());
                    out.push('\n');
                    in_item = false;
                }
                b"list" => out.push('\n'),
                b"note" if in_note => {
                    out.push_str("> Note: ");
                    out.push_str(current.trim());
                    out.push_str("\n\n");
                    in_note = false;
                }
                b"footnote" if in_footnote => {
                    out.push_str("[^note]: ");
                    out.push_str(current.trim());
                    out.push_str("\n\n");
                    in_footnote = false;
                }
                b"reference" if in_reference => {
                    out.push_str("- ");
                    out.push_str(current.trim());
                    out.push('\n');
                    in_reference = false;
                }
                b"references" => out.push('\n'),
                b"definition" if in_definition => {
                    out.push_str("- ");
                    if !definition_term.is_empty() {
                        out.push_str(&definition_term);
                        out.push_str(": ");
                    }
                    out.push_str(current.trim());
                    out.push('\n');
                    in_definition = false;
                }
                b"definitionList" => out.push('\n'),
                b"cell" if in_cell => {
                    current_row.push(current.trim().to_string());
                    in_cell = false;
                }
                b"row" if in_row => {
                    table_rows.push(current_row.clone());
                    in_row = false;
                }
                b"table" if in_table => {
                    out.push_str(&table_to_markdown(&table_rows));
                    in_table = false;
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            _ => {}
        }
    }
    out.trim_end().to_string()
}

fn table_to_markdown(rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("| ");
    out.push_str(&rows[0].join(" | "));
    out.push_str(" |\n| ");
    out.push_str(&vec!["---"; rows[0].len()].join(" | "));
    out.push_str(" |\n");
    for row in rows.iter().skip(1) {
        out.push_str("| ");
        out.push_str(&row.join(" | "));
        out.push_str(" |\n");
    }
    out.push('\n');
    out
}

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

fn start_capture(current: &mut String, capture: &mut Option<&'static str>, kind: &'static str) {
    current.clear();
    *capture = Some(kind);
}

fn heading_node(depth: usize, value: &str) -> MarkdownNode {
    MarkdownNode {
        node_type: "heading",
        value: None,
        depth: Some(depth.clamp(1, 6)),
        lang: None,
        ordered: None,
        children: vec![text_node(value)],
    }
}

fn paragraph_node(value: &str) -> MarkdownNode {
    MarkdownNode {
        node_type: "paragraph",
        value: None,
        depth: None,
        lang: None,
        ordered: None,
        children: vec![text_node(value)],
    }
}

fn blockquote_node(value: &str) -> MarkdownNode {
    MarkdownNode {
        node_type: "blockquote",
        value: None,
        depth: None,
        lang: None,
        ordered: None,
        children: vec![paragraph_node(value)],
    }
}

fn list_item_node(value: &str) -> MarkdownNode {
    MarkdownNode {
        node_type: "listItem",
        value: None,
        depth: None,
        lang: None,
        ordered: None,
        children: vec![paragraph_node(value)],
    }
}

fn table_node(rows: &[Vec<String>]) -> MarkdownNode {
    MarkdownNode {
        node_type: "table",
        value: None,
        depth: None,
        lang: None,
        ordered: None,
        children: rows
            .iter()
            .map(|row| MarkdownNode {
                node_type: "tableRow",
                value: None,
                depth: None,
                lang: None,
                ordered: None,
                children: row
                    .iter()
                    .map(|cell| MarkdownNode {
                        node_type: "tableCell",
                        value: None,
                        depth: None,
                        lang: None,
                        ordered: None,
                        children: vec![text_node(cell)],
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn value_node(node_type: &'static str, value: &str, lang: Option<String>) -> MarkdownNode {
    MarkdownNode {
        node_type,
        value: Some(value.to_string()),
        depth: None,
        lang,
        ordered: None,
        children: Vec::new(),
    }
}

fn text_node(value: &str) -> MarkdownNode {
    value_node("text", value, None)
}

fn attr_value(e: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .map(|a| String::from_utf8_lossy(a.value.as_ref()).to_string())
}
