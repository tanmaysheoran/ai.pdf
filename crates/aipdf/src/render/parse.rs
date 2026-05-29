use quick_xml::events::Event;
use quick_xml::Reader;

// ── Document element model ────────────────────────────────────────────────────

#[derive(Debug)]
pub(super) enum DocElem {
    DocTitle(String),
    Heading { level: usize, text: String, id: Option<String> },
    Paragraph { text: String, id: Option<String> },
    Table { caption: Option<String>, rows: Vec<Vec<String>>, id: Option<String> },
    CodeBlock { language: Option<String>, text: String, id: Option<String> },
    List { ordered: bool, items: Vec<String>, id: Option<String> },
    Citation { text: String, id: Option<String> },
    Note { text: String, id: Option<String> },
    Figure { alt: String, src: String, caption: Option<String>, id: Option<String> },
}

/// Page + bounding box recorded for a block during layout (PDF user-space
/// points, page-local, origin bottom-left). Written back into the semantic XML.
pub(super) struct BlockCoord {
    pub(super) id: String,
    pub(super) page: usize,
    pub(super) bbox: (f32, f32, f32, f32),
}

pub(super) fn parse_elements(xml: &str) -> Vec<DocElem> {
    let mut elems = Vec::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    // metadata title
    let mut in_meta_title = false;
    let mut in_meta = false;
    let mut meta_title = String::new();

    // section tracking
    let mut section_level = 1usize;

    // capture state
    let mut in_title = false;
    let mut in_para = false;
    let mut in_citation = false;
    let mut in_equation = false;
    let mut in_note = false;
    let mut _in_footnote = false;
    let mut in_item = false;
    let mut in_reference = false;
    let mut in_code = false;
    let mut code_lang: Option<String> = None;
    let mut current = String::new();
    // Captured `id` of the block currently being assembled.
    let mut block_id: Option<String> = None;
    let mut list_id: Option<String> = None;
    let mut table_id: Option<String> = None;
    let mut fig_id: Option<String> = None;

    // list state
    let mut in_list = false;
    let mut list_ordered = false;
    let mut list_items: Vec<String> = Vec::new();

    // table state
    let mut in_table = false;
    let mut table_caption: Option<String> = None;
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut in_row = false;
    let mut current_row: Vec<String> = Vec::new();
    let mut in_cell = false;
    let mut in_table_caption = false;

    // figure state
    let mut in_figure = false;
    let mut fig_src = String::new();
    let mut fig_alt = String::new();
    let mut fig_caption: Option<String> = None;
    let mut in_fig_caption = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "metadata" => in_meta = true,
                    "section" if !in_meta => {
                        section_level = attr_val(&e, b"level")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(1);
                    }
                    "title" if in_meta => {
                        in_meta_title = true;
                        meta_title.clear();
                    }
                    "title" if !in_meta && !in_table => {
                        in_title = true;
                        block_id = attr_val(&e, b"id");
                        current.clear();
                    }
                    "paragraph" => {
                        in_para = true;
                        block_id = attr_val(&e, b"id");
                        current.clear();
                    }
                    "citation" => {
                        in_citation = true;
                        block_id = attr_val(&e, b"id");
                        current.clear();
                    }
                    "equation" => {
                        in_equation = true;
                        block_id = attr_val(&e, b"id");
                        current.clear();
                    }
                    "note" | "footnote" if !in_table => {
                        in_note = true;
                        block_id = attr_val(&e, b"id");
                        current.clear();
                    }
                    "codeBlock" => {
                        in_code = true;
                        code_lang = attr_val(&e, b"language");
                        block_id = attr_val(&e, b"id");
                        current.clear();
                    }
                    "list" => {
                        in_list = true;
                        list_id = attr_val(&e, b"id");
                        list_ordered = attr_val(&e, b"type")
                            .map(|t| t == "ordered")
                            .unwrap_or(false);
                        list_items.clear();
                    }
                    "item" | "reference" if in_list => {
                        in_item = true;
                        current.clear();
                    }
                    "references" => {
                        in_list = true;
                        list_id = attr_val(&e, b"id");
                        list_ordered = false;
                        list_items.clear();
                    }
                    "reference" if !in_list => {
                        in_reference = true;
                        block_id = attr_val(&e, b"id");
                        current.clear();
                    }
                    "table" if !in_meta => {
                        in_table = true;
                        table_id = attr_val(&e, b"id");
                        table_caption = None;
                        table_rows.clear();
                    }
                    "caption" if in_table => {
                        in_table_caption = true;
                        current.clear();
                    }
                    "row" if in_table => {
                        in_row = true;
                        current_row.clear();
                    }
                    "cell" if in_row => {
                        in_cell = true;
                        current.clear();
                    }
                    "figure" => {
                        in_figure = true;
                        fig_id = attr_val(&e, b"id");
                        fig_src.clear();
                        fig_alt.clear();
                        fig_caption = None;
                    }
                    "caption" if in_figure => {
                        in_fig_caption = true;
                        current.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "image" && in_figure {
                    fig_src = attr_val(&e, b"src").unwrap_or_default();
                    fig_alt = attr_val(&e, b"alt").unwrap_or_default();
                }
            }
            Ok(Event::Text(t)) => {
                let text = t.unescape().unwrap_or_default().to_string();
                if in_meta_title {
                    meta_title.push_str(&text);
                } else if in_cell || in_item || in_para || in_title || in_citation
                    || in_equation || in_note || in_code || in_reference
                    || in_table_caption || in_fig_caption
                {
                    current.push_str(text.trim());
                }
            }
            Ok(Event::CData(t)) => {
                if in_code || in_equation {
                    current.push_str(String::from_utf8_lossy(&t).trim());
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "metadata" => {
                        in_meta = false;
                        if !meta_title.trim().is_empty() {
                            elems.push(DocElem::DocTitle(meta_title.trim().to_string()));
                        }
                    }
                    "title" if in_meta_title => {
                        in_meta_title = false;
                    }
                    "title" if in_title => {
                        elems.push(DocElem::Heading {
                            level: section_level,
                            text: current.trim().to_string(),
                            id: block_id.take(),
                        });
                        in_title = false;
                    }
                    "paragraph" if in_para => {
                        if !current.trim().is_empty() {
                            elems.push(DocElem::Paragraph {
                                text: current.trim().to_string(),
                                id: block_id.take(),
                            });
                        }
                        in_para = false;
                    }
                    "citation" if in_citation => {
                        elems.push(DocElem::Citation {
                            text: current.trim().to_string(),
                            id: block_id.take(),
                        });
                        in_citation = false;
                    }
                    "equation" if in_equation => {
                        elems.push(DocElem::Paragraph {
                            text: format!("[ {} ]", current.trim()),
                            id: block_id.take(),
                        });
                        in_equation = false;
                    }
                    "note" | "footnote" if in_note => {
                        elems.push(DocElem::Note {
                            text: current.trim().to_string(),
                            id: block_id.take(),
                        });
                        in_note = false;
                        _in_footnote = false;
                    }
                    "codeBlock" if in_code => {
                        elems.push(DocElem::CodeBlock {
                            language: code_lang.take(),
                            text: current.trim().to_string(),
                            id: block_id.take(),
                        });
                        in_code = false;
                    }
                    "item" | "reference" if in_item => {
                        list_items.push(current.trim().to_string());
                        in_item = false;
                    }
                    "list" | "references" if in_list => {
                        if !list_items.is_empty() {
                            elems.push(DocElem::List {
                                ordered: list_ordered,
                                items: list_items.clone(),
                                id: list_id.take(),
                            });
                        }
                        in_list = false;
                    }
                    "reference" if in_reference => {
                        if !current.trim().is_empty() {
                            elems.push(DocElem::Paragraph {
                                text: current.trim().to_string(),
                                id: block_id.take(),
                            });
                        }
                        in_reference = false;
                    }
                    "caption" if in_table_caption => {
                        table_caption = Some(current.trim().to_string());
                        in_table_caption = false;
                    }
                    "cell" if in_cell => {
                        current_row.push(current.trim().to_string());
                        in_cell = false;
                    }
                    "row" if in_row => {
                        if !current_row.is_empty() {
                            table_rows.push(current_row.clone());
                        }
                        in_row = false;
                    }
                    "table" if in_table => {
                        if !table_rows.is_empty() {
                            elems.push(DocElem::Table {
                                caption: table_caption.take(),
                                rows: table_rows.clone(),
                                id: table_id.take(),
                            });
                        }
                        in_table = false;
                    }
                    "caption" if in_fig_caption => {
                        fig_caption = Some(current.trim().to_string());
                        in_fig_caption = false;
                    }
                    "figure" if in_figure => {
                        elems.push(DocElem::Figure {
                            alt: fig_alt.clone(),
                            src: fig_src.clone(),
                            caption: fig_caption.take(),
                            id: fig_id.take(),
                        });
                        in_figure = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
    }
    elems
}

pub(super) fn attr_val(e: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .map(|a| String::from_utf8_lossy(a.value.as_ref()).to_string())
}
