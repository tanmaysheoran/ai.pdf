use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

#[derive(Debug, Clone, Default)]
struct SectionRecord {
    id: String,
    level: String,
    page: String,
    role: String,
    title: String,
}

#[derive(Debug, Clone, Default)]
struct BlockRecord {
    id: String,
    kind: String,
    section_id: String,
    level: String,
    page: String,
    bbox: String,
    role: String,
    text: String,
}

#[derive(Debug, Clone, Default)]
struct TableRecord {
    id: String,
    page: String,
    bbox: String,
    caption: String,
    rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Default)]
struct FigureRecord {
    id: String,
    page: String,
    bbox: String,
    caption: String,
    alt: String,
    source: String,
}

#[derive(Debug, Clone, Default)]
struct ReferenceRecord {
    id: String,
    ref_type: String,
    text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureKind {
    MetadataTitle,
    Block,
    TableCaption,
    TableCell,
    FigureCaption,
    FigureAltText,
    Reference,
}

#[derive(Debug, Clone)]
struct Capture {
    kind: CaptureKind,
    tag: String,
    text: String,
    block: Option<BlockRecord>,
    reference: Option<ReferenceRecord>,
}

#[derive(Debug, Clone, Default)]
struct SectionContext {
    id: String,
    level: String,
    page: String,
}

/// Converts AIPDF semantic XML into a compact ONTO-style columnar text view.
///
/// The XML remains the canonical embedded representation; this is a derived
/// export intended for token-efficient LLM ingestion.
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

fn render_onto(
    version: &str,
    title: &str,
    sections: &[SectionRecord],
    blocks: &[BlockRecord],
    tables: &[TableRecord],
    figures: &[FigureRecord],
    references: &[ReferenceRecord],
) -> String {
    let mut out = String::new();
    out.push_str("Document[1]:\n");
    field(&mut out, "version", version);
    field(&mut out, "title", title);
    field(&mut out, "source_format", "aipdf.semantic.xml");

    out.push('\n');
    out.push_str(&format!("Sections[{}]:\n", sections.len()));
    column(&mut out, "id", sections.iter().map(|s| s.id.as_str()));
    column(&mut out, "level", sections.iter().map(|s| s.level.as_str()));
    column(&mut out, "page", sections.iter().map(|s| s.page.as_str()));
    column(&mut out, "role", sections.iter().map(|s| s.role.as_str()));
    column(&mut out, "title", sections.iter().map(|s| s.title.as_str()));

    out.push('\n');
    out.push_str(&format!("Blocks[{}]:\n", blocks.len()));
    column(&mut out, "id", blocks.iter().map(|b| b.id.as_str()));
    column(&mut out, "kind", blocks.iter().map(|b| b.kind.as_str()));
    column(
        &mut out,
        "section_id",
        blocks.iter().map(|b| b.section_id.as_str()),
    );
    column(&mut out, "level", blocks.iter().map(|b| b.level.as_str()));
    column(&mut out, "page", blocks.iter().map(|b| b.page.as_str()));
    column(&mut out, "bbox", blocks.iter().map(|b| b.bbox.as_str()));
    column(&mut out, "role", blocks.iter().map(|b| b.role.as_str()));
    column(&mut out, "text", blocks.iter().map(|b| b.text.as_str()));

    out.push('\n');
    out.push_str(&format!("Tables[{}]:\n", tables.len()));
    column(&mut out, "id", tables.iter().map(|t| t.id.as_str()));
    column(&mut out, "page", tables.iter().map(|t| t.page.as_str()));
    column(&mut out, "bbox", tables.iter().map(|t| t.bbox.as_str()));
    column(
        &mut out,
        "caption",
        tables.iter().map(|t| t.caption.as_str()),
    );
    column_raw(
        &mut out,
        "rows",
        tables.iter().map(|t| t.rows_as_onto()).collect::<Vec<_>>(),
    );

    out.push('\n');
    out.push_str(&format!("Figures[{}]:\n", figures.len()));
    column(&mut out, "id", figures.iter().map(|f| f.id.as_str()));
    column(&mut out, "page", figures.iter().map(|f| f.page.as_str()));
    column(&mut out, "bbox", figures.iter().map(|f| f.bbox.as_str()));
    column(
        &mut out,
        "caption",
        figures.iter().map(|f| f.caption.as_str()),
    );
    column(&mut out, "alt", figures.iter().map(|f| f.alt.as_str()));
    column(
        &mut out,
        "source",
        figures.iter().map(|f| f.source.as_str()),
    );

    out.push('\n');
    out.push_str(&format!("References[{}]:\n", references.len()));
    column(&mut out, "id", references.iter().map(|r| r.id.as_str()));
    column(
        &mut out,
        "type",
        references.iter().map(|r| r.ref_type.as_str()),
    );
    column(&mut out, "text", references.iter().map(|r| r.text.as_str()));

    out.trim_end().to_string()
}

impl TableRecord {
    fn rows_as_onto(&self) -> String {
        self.rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| encode_array_scalar(cell))
                    .collect::<Vec<_>>()
                    .join("^")
            })
            .collect::<Vec<_>>()
            .join("|")
    }
}

fn field(out: &mut String, name: &str, value: &str) {
    out.push_str("    ");
    out.push_str(name);
    out.push_str(": ");
    out.push_str(&encode_scalar(value));
    out.push('\n');
}

fn column<'a, I, S>(out: &mut String, name: &str, values: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str> + 'a,
{
    out.push_str("    ");
    out.push_str(name);
    out.push_str(": ");
    out.push_str(
        &values
            .into_iter()
            .map(|v| encode_scalar(v.as_ref()))
            .collect::<Vec<_>>()
            .join("|"),
    );
    out.push('\n');
}

fn column_raw(out: &mut String, name: &str, values: Vec<String>) {
    out.push_str("    ");
    out.push_str(name);
    out.push_str(": ");
    out.push_str(&values.join("|"));
    out.push('\n');
}

fn encode_scalar(value: &str) -> String {
    let normalized = normalize(value);
    let escaped = normalized
        .replace('`', "'")
        .replace('|', "/")
        .replace('^', ";");
    if escaped.contains('\n') || escaped.starts_with(' ') || escaped.ends_with(' ') {
        format!("`{}`", escaped.trim())
    } else {
        escaped
    }
}

fn encode_array_scalar(value: &str) -> String {
    encode_scalar(value)
}

fn normalize(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn default_role(name: &str) -> String {
    match name {
        "title" => "heading",
        "paragraph" => "body",
        "caption" => "caption",
        "equation" => "equation",
        "citation" => "citation",
        "item" => "list-item",
        "note" => "note",
        "footnote" => "footnote",
        "definition" => "definition",
        "codeBlock" => "code",
        "annotation" => "annotation",
        _ => name,
    }
    .to_string()
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

#[cfg(test)]
mod tests {
    use super::xml_to_onto;

    #[test]
    fn exports_minimal_onto() {
        let onto = xml_to_onto(include_str!("../../../samples/minimal.xml"));
        assert!(onto.contains("Document[1]:"));
        assert!(onto.contains("Blocks["));
        assert!(onto.contains("Tables[1]:"));
        assert!(onto.contains("Introduction"));
        assert!(onto.contains("Target^Limit|Ideal overhead^<3%"));
    }

    #[test]
    fn exports_maximal_onto() {
        let source = include_str!("../../../samples/maximal.xml");
        let xml = source
            .split("```xml")
            .nth(1)
            .and_then(|s| s.split("```").next())
            .unwrap_or(source);
        let onto = xml_to_onto(xml);
        assert!(onto.contains("Figures[1]:"));
        assert!(onto.contains("Tables[1]:"));
        assert!(onto.contains("References[2]:"));
        assert!(onto.contains("Mathematical Compression Model"));
        assert!(onto.contains("C = (S_o - S_c) / S_o * 100"));
    }
}
