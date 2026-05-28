use crate::{security::sanitize_xml, AipdfError, Result};
use quick_xml::events::Event;
use quick_xml::Reader;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticBlock {
    pub kind: String,
    pub id: Option<String>,
    pub page: Option<u32>,
    pub bbox: Option<String>,
    pub text: String,
}

pub fn validate_xml(xml: &str) -> Result<()> {
    let xml = sanitize_xml(xml)?;
    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(true);
    let mut depth = 0usize;
    let mut root_seen = false;
    let mut section_seen = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                depth += 1;
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if depth == 1 {
                    if name != "document" {
                        return Err(AipdfError::InvalidXml(
                            "root element must be <document>".to_string(),
                        ));
                    }
                    root_seen = true;
                    let has_version = e
                        .attributes()
                        .flatten()
                        .find(|a| a.key.as_ref() == b"version")
                        .map(|a| !a.value.as_ref().is_empty())
                        .unwrap_or(false);
                    if !has_version {
                        return Err(AipdfError::InvalidXml(
                            "document version must be present".to_string(),
                        ));
                    }
                }
                if name == "section" {
                    section_seen = true;
                    let has_id = e.attributes().flatten().any(|a| a.key.as_ref() == b"id");
                    if !has_id {
                        return Err(AipdfError::InvalidXml(
                            "section elements require stable id attributes".to_string(),
                        ));
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                if e.name().as_ref() == b"section" {
                    return Err(AipdfError::InvalidXml(
                        "section elements must contain semantic blocks".to_string(),
                    ));
                }
            }
            Ok(Event::End(_)) => depth = depth.saturating_sub(1),
            Ok(Event::DocType(_)) => {
                return Err(AipdfError::InvalidXml(
                    "DOCTYPE declarations are not allowed".to_string(),
                ))
            }
            Ok(Event::PI(_)) => {
                return Err(AipdfError::InvalidXml(
                    "processing instructions are not allowed".to_string(),
                ))
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(AipdfError::InvalidXml(err.to_string())),
            _ => {}
        }
    }

    if !root_seen || !section_seen {
        return Err(AipdfError::InvalidXml(
            "document must contain at least one section".to_string(),
        ));
    }
    Ok(())
}


pub fn get_reading_order(xml: &str) -> Result<Vec<SemanticBlock>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut blocks = Vec::new();
    let mut current: Option<SemanticBlock> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let kind = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if matches!(
                    kind.as_str(),
                    "title"
                        | "paragraph"
                        | "caption"
                        | "equation"
                        | "citation"
                        | "cell"
                        | "item"
                        | "codeBlock"
                        | "reference"
                        | "footnote"
                        | "note"
                ) {
                    let mut block = SemanticBlock {
                        kind,
                        id: None,
                        page: None,
                        bbox: None,
                        text: String::new(),
                    };
                    for attr in e.attributes().flatten() {
                        let value = String::from_utf8_lossy(attr.value.as_ref()).to_string();
                        match attr.key.as_ref() {
                            b"id" => block.id = Some(value),
                            b"page" => block.page = value.parse().ok(),
                            b"bbox" => block.bbox = Some(value),
                            _ => {}
                        }
                    }
                    current = Some(block);
                }
            }
            Ok(Event::Text(t)) => {
                if let Some(block) = current.as_mut() {
                    block.text.push_str(&t.unescape().unwrap_or_default());
                }
            }
            Ok(Event::CData(t)) => {
                if let Some(block) = current.as_mut() {
                    block.text.push_str(String::from_utf8_lossy(&t).trim());
                }
            }
            Ok(Event::End(e)) => {
                if let Some(block) = current.take() {
                    if e.name().as_ref() == block.kind.as_bytes() {
                        blocks.push(block);
                    } else {
                        current = Some(block);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(AipdfError::InvalidXml(err.to_string())),
            _ => {}
        }
    }
    Ok(blocks)
}

pub fn get_tables(xml: &str) -> Result<Vec<String>> {
    collect_element_text(xml, "table")
}

pub fn find_citations(xml: &str) -> Result<Vec<String>> {
    collect_element_text(xml, "citation")
}

fn collect_element_text(xml: &str, element: &str) -> Result<Vec<String>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut capture = false;
    let mut text = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.name().as_ref() == element.as_bytes() => {
                capture = true;
                depth = 1;
                text.clear();
            }
            Ok(Event::Start(_)) if capture => depth += 1,
            Ok(Event::Text(t)) if capture => {
                let piece = t.unescape().unwrap_or_default();
                if !text.is_empty() && !piece.trim().is_empty() {
                    text.push(' ');
                }
                text.push_str(piece.trim());
            }
            Ok(Event::End(_)) if capture => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    out.push(text.trim().to_string());
                    capture = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(AipdfError::InvalidXml(err.to_string())),
            _ => {}
        }
    }
    Ok(out)
}
