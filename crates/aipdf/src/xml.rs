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

/// Major schema version this build implements. Version negotiation is
/// forward-compatible within a major version: any `1.x` payload is accepted and
/// unknown elements/attributes are ignored, so a v1.0 reader can open a v1.3
/// file. A different major version is rejected — readers encountering one
/// should fall back to treating the file as an ordinary PDF (see docs/spec.md).
pub const SUPPORTED_MAJOR_VERSION: u32 = 1;

fn check_supported_version(version: &str) -> Result<()> {
    let major: u32 = version
        .split('.')
        .next()
        .unwrap_or("")
        .parse()
        .map_err(|_| {
            AipdfError::InvalidXml(format!("malformed document version `{version}`"))
        })?;
    if major != SUPPORTED_MAJOR_VERSION {
        return Err(AipdfError::InvalidXml(format!(
            "unsupported document version `{version}`: this build supports {SUPPORTED_MAJOR_VERSION}.x"
        )));
    }
    Ok(())
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
                    let version = e
                        .attributes()
                        .flatten()
                        .find(|a| a.key.as_ref() == b"version")
                        .map(|a| String::from_utf8_lossy(a.value.as_ref()).trim().to_string())
                        .unwrap_or_default();
                    if version.is_empty() {
                        return Err(AipdfError::InvalidXml(
                            "document version must be present".to_string(),
                        ));
                    }
                    check_supported_version(&version)?;
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

#[cfg(test)]
mod version_tests {
    use super::*;

    fn doc(version: &str) -> String {
        format!(
            r#"<document version="{version}"><section id="s1"><paragraph>x</paragraph></section></document>"#
        )
    }

    #[test]
    fn accepts_any_1_x() {
        for v in ["1.0", "1.3", "1.99"] {
            validate_xml(&doc(v)).unwrap_or_else(|e| panic!("{v} should validate: {e}"));
        }
    }

    #[test]
    fn rejects_other_majors_and_malformed() {
        assert!(validate_xml(&doc("2.0")).is_err());
        assert!(validate_xml(&doc("0.9")).is_err());
        assert!(validate_xml(&doc("abc")).is_err());
        assert!(validate_xml(&doc("")).is_err());
    }
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
