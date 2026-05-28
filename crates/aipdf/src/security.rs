use crate::{AipdfError, Result};

const DISALLOWED_MARKERS: &[&str] = &[
    "<!DOCTYPE",
    "<?xml-stylesheet",
    "<?processing",
    "<script",
    "/JavaScript",
    "/Launch",
    "prompt:",
    "system prompt",
    "model directive",
];

pub fn sanitize_xml(xml: &str) -> Result<String> {
    let trimmed = xml.trim_start_matches('\u{feff}').trim().to_string();
    for marker in DISALLOWED_MARKERS {
        if trimmed
            .to_ascii_lowercase()
            .contains(&marker.to_ascii_lowercase())
        {
            return Err(AipdfError::InvalidXml(format!(
                "disallowed marker `{marker}`"
            )));
        }
    }
    if trimmed.len() > 16 * 1024 * 1024 {
        return Err(AipdfError::InvalidXml(
            "semantic XML exceeds 16 MiB safety limit".to_string(),
        ));
    }
    Ok(trimmed)
}
