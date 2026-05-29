use quick_xml::events::Event;
use quick_xml::Reader;

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

pub(super) fn table_to_markdown(rows: &[Vec<String>]) -> String {
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
