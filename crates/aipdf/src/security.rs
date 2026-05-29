use crate::{AipdfError, Result};

/// Active-content / structural markers that must never appear as *markup* in the
/// semantic layer. These are real injection vectors (executable actions, DTDs,
/// processing instructions). Note that legitimate body text is XML-escaped, so
/// e.g. the literal text "<script" becomes "&lt;script" and does not match here
/// — only actual markup trips these.
///
/// Deliberately NOT included: natural-language phrases like "system prompt" or
/// "prompt:". The threat model treats XML text as data, never as instructions
/// (see docs/security.md), and the visible PDF layer already carries the same
/// words — so banning them would only reject legitimate documents (e.g. an AI
/// paper, or ingesting a PDF that discusses prompting) for no security gain.
const DISALLOWED_MARKERS: &[&str] = &[
    "<!DOCTYPE",
    "<?xml-stylesheet",
    "<?processing",
    "<script",
    "/JavaScript",
    "/Launch",
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
