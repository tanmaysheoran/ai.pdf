use super::encode::{default_role, normalize};
use super::render_onto::render_onto;
use super::{
    BlockRecord, Capture, CaptureKind, FigureRecord, ReferenceRecord, SectionContext, SectionRecord,
    TableRecord,
};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

pub fn xml_to_onto(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut document_version = String::new();
    let mut document_title = String::new();
    let mut sections: Vec<SectionRecord> = Vec::new();
    let mut blocks: Vec<BlockRecord> = Vec::new();
    let mut tables: Vec<TableRecord> = Vec::new();
    let mut figures: Vec<FigureRecord> = Vec::new();
    let mut references: Vec<ReferenceRecord> = Vec::new();

    let mut element_stack: Vec<String> = Vec::new();
    let mut section_stack: Vec<SectionContext> = Vec::new();
    let mut current_table: Option<TableRecord> = None;
    let mut current_row: Vec<String> = Vec::new();
    let mut current_figure: Option<FigureRecord> = None;
    let mut capture: Option<Capture> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = tag_name(&e);
                element_stack.push(name.clone());
                match name.as_str() {
                    "document" => {
                        document_version = attr_value(&e, b"version").unwrap_or_default();
                    }
                    "section" => {
                        let id = attr_value(&e, b"id").unwrap_or_default();
                        let level = attr_value(&e, b"level").unwrap_or_default();
                        let page = attr_value(&e, b"page")
                            .or_else(|| attr_value(&e, b"pageStart"))
                            .unwrap_or_default();
                        let role = attr_value(&e, b"role")
                            .or_else(|| attr_value(&e, b"semanticRole"))
                            .unwrap_or_default();
                        sections.push(SectionRecord {
                            id: id.clone(),
                            level: level.clone(),
                            page: page.clone(),
                            role,
                            title: String::new(),
                        });
                        section_stack.push(SectionContext { id, level, page });
                    }
                    "appendix" => {
                        let id = attr_value(&e, b"id").unwrap_or_default();
                        sections.push(SectionRecord {
                            id: id.clone(),
                            level: "appendix".to_string(),
                            page: attr_value(&e, b"page").unwrap_or_default(),
                            role: "appendix".to_string(),
                            title: String::new(),
                        });
                        section_stack.push(SectionContext {
                            id,
                            level: "appendix".to_string(),
                            page: String::new(),
                        });
                    }
                    "title" if is_inside(&element_stack, "metadata") => {
                        capture = Some(new_capture(CaptureKind::MetadataTitle, &name));
                    }
                    "title" | "paragraph" | "caption" | "equation" | "citation" | "item"
                    | "note" | "footnote" | "definition" | "codeBlock" | "annotation" => {
                        if current_table.is_some() && name == "caption" {
                            capture = Some(new_capture(CaptureKind::TableCaption, &name));
                        } else if current_figure.is_some() && name == "caption" {
                            capture = Some(new_capture(CaptureKind::FigureCaption, &name));
                        } else if capture.is_none() {
                            let ctx = section_stack.last().cloned().unwrap_or_default();
                            let kind = if name == "definition" {
                                "definition".to_string()
                            } else {
                                name.clone()
                            };
                            let role =
                                attr_value(&e, b"role").unwrap_or_else(|| default_role(&name));
                            let mut text_prefix = String::new();
                            if name == "definition" {
                                if let Some(term) = attr_value(&e, b"term") {
                                    text_prefix = format!("{term}: ");
                                }
                            }
                            capture = Some(Capture {
                                kind: CaptureKind::Block,
                                tag: name.clone(),
                                text: text_prefix,
                                block: Some(BlockRecord {
                                    id: attr_value(&e, b"id").unwrap_or_default(),
                                    kind,
                                    section_id: ctx.id,
                                    level: ctx.level,
                                    page: attr_value(&e, b"page").unwrap_or(ctx.page),
                                    bbox: attr_value(&e, b"bbox").unwrap_or_default(),
                                    role,
                                    text: String::new(),
                                }),
                                reference: None,
                            });
                        }
                    }
                    "table" => {
                        current_table = Some(TableRecord {
                            id: attr_value(&e, b"id").unwrap_or_default(),
                            page: attr_value(&e, b"page").unwrap_or_default(),
                            bbox: attr_value(&e, b"bbox").unwrap_or_default(),
                            caption: String::new(),
                            rows: Vec::new(),
                        });
                    }
                    "row" if current_table.is_some() => current_row.clear(),
                    "cell" if current_table.is_some() => {
                        capture = Some(new_capture(CaptureKind::TableCell, &name));
                    }
                    "figure" => {
                        current_figure = Some(FigureRecord {
                            id: attr_value(&e, b"id").unwrap_or_default(),
                            page: attr_value(&e, b"page").unwrap_or_default(),
                            bbox: attr_value(&e, b"bbox").unwrap_or_default(),
                            caption: String::new(),
                            alt: String::new(),
                            source: attr_value(&e, b"source").unwrap_or_default(),
                        });
                    }
                    "image" if current_figure.is_some() => {
                        if let Some(figure) = current_figure.as_mut() {
                            figure.source =
                                attr_value(&e, b"src").unwrap_or_else(|| figure.source.clone());
                            figure.alt =
                                attr_value(&e, b"alt").unwrap_or_else(|| figure.alt.clone());
                        }
                    }
                    "alt" | "altText" if current_figure.is_some() => {
                        capture = Some(new_capture(CaptureKind::FigureAltText, &name));
                    }
                    "reference" => {
                        capture = Some(Capture {
                            kind: CaptureKind::Reference,
                            tag: name.clone(),
                            text: String::new(),
                            block: None,
                            reference: Some(ReferenceRecord {
                                id: attr_value(&e, b"id").unwrap_or_default(),
                                ref_type: attr_value(&e, b"type").unwrap_or_default(),
                                text: String::new(),
                            }),
                        });
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                if e.name().as_ref() == b"image" {
                    if let Some(figure) = current_figure.as_mut() {
                        figure.source =
                            attr_value(&e, b"src").unwrap_or_else(|| figure.source.clone());
                        figure.alt = attr_value(&e, b"alt").unwrap_or_else(|| figure.alt.clone());
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if let Some(active) = capture.as_mut() {
                    active.text.push_str(&t.unescape().unwrap_or_default());
                }
            }
            Ok(Event::CData(t)) => {
                if let Some(active) = capture.as_mut() {
                    active.text.push_str(String::from_utf8_lossy(&t).as_ref());
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if let Some(active) = capture.take() {
                    if active.tag == name {
                        finish_capture(
                            active,
                            &mut document_title,
                            &mut sections,
                            &mut blocks,
                            &mut current_table,
                            &mut current_row,
                            &mut current_figure,
                            &mut references,
                        );
                    } else {
                        capture = Some(active);
                    }
                }

                match name.as_str() {
                    "row" if current_table.is_some() => {
                        if let Some(table) = current_table.as_mut() {
                            table.rows.push(current_row.clone());
                        }
                        current_row.clear();
                    }
                    "table" => {
                        if let Some(table) = current_table.take() {
                            tables.push(table);
                        }
                    }
                    "figure" => {
                        if let Some(figure) = current_figure.take() {
                            figures.push(figure);
                        }
                    }
                    "section" | "appendix" => {
                        section_stack.pop();
                    }
                    _ => {}
                }
                element_stack.pop();
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
    }

    render_onto(
        &document_version,
        &document_title,
        &sections,
        &blocks,
        &tables,
        &figures,
        &references,
    )
}

#[allow(clippy::too_many_arguments)]
fn finish_capture(
    mut active: Capture,
    document_title: &mut String,
    sections: &mut [SectionRecord],
    blocks: &mut Vec<BlockRecord>,
    current_table: &mut Option<TableRecord>,
    current_row: &mut Vec<String>,
    current_figure: &mut Option<FigureRecord>,
    references: &mut Vec<ReferenceRecord>,
) {
    let text = normalize(&active.text);
    match active.kind {
        CaptureKind::MetadataTitle => *document_title = text,
        CaptureKind::Block => {
            if let Some(mut block) = active.block.take() {
                block.text = text;
                if block.kind == "title" {
                    if let Some(section) = sections.last_mut() {
                        if section.title.is_empty() {
                            section.title = block.text.clone();
                        }
                    }
                }
                blocks.push(block);
            }
        }
        CaptureKind::TableCaption => {
            if let Some(table) = current_table.as_mut() {
                table.caption = text;
            }
        }
        CaptureKind::TableCell => current_row.push(text),
        CaptureKind::FigureCaption => {
            if let Some(figure) = current_figure.as_mut() {
                figure.caption = text;
            }
        }
        CaptureKind::FigureAltText => {
            if let Some(figure) = current_figure.as_mut() {
                figure.alt = text;
            }
        }
        CaptureKind::Reference => {
            if let Some(mut reference) = active.reference.take() {
                reference.text = text;
                references.push(reference);
            }
        }
    }
}

fn new_capture(kind: CaptureKind, tag: &str) -> Capture {
    Capture {
        kind,
        tag: tag.to_string(),
        text: String::new(),
        block: None,
        reference: None,
    }
}

fn tag_name(e: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(e.name().as_ref()).to_string()
}

fn attr_value(e: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .map(|a| String::from_utf8_lossy(a.value.as_ref()).to_string())
}

fn is_inside(stack: &[String], tag: &str) -> bool {
    stack.iter().rev().skip(1).any(|item| item == tag)
}
