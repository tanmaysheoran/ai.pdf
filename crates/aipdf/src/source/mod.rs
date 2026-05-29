use crate::{security::sanitize_xml, xml::validate_xml, AipdfError, Result};
use std::path::Path;

mod html;
mod html_walk;
mod markdown;
mod typst;

pub(crate) use html_walk::html_to_xml;
pub(crate) use markdown::markdown_to_xml;
pub(crate) use typst::typst_to_xml;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Xml,
    Markdown,
    Html,
    Typst,
}

impl SourceKind {
    pub fn from_path(path: &Path) -> Result<Self> {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
        {
            "xml" => Ok(Self::Xml),
            "md" | "markdown" => Ok(Self::Markdown),
            "html" | "htm" => Ok(Self::Html),
            "typ" | "typst" => Ok(Self::Typst),
            other => Err(AipdfError::InvalidXml(format!(
                "unsupported input extension `{other}`"
            ))),
        }
    }
}

pub fn semantic_xml_from_source(input: &str, kind: SourceKind) -> Result<String> {
    let xml = match kind {
        SourceKind::Xml => sanitize_xml(&extract_xml_payload(input))?,
        SourceKind::Markdown => markdown_to_xml(input),
        SourceKind::Html => html_to_xml(input),
        SourceKind::Typst => typst_to_xml(input),
    };
    validate_xml(&xml)?;
    Ok(xml)
}

pub(crate) fn extract_xml_payload(input: &str) -> String {
    let raw = if let Some(start) = input.find("```xml") {
        let after_start = &input[start + "```xml".len()..];
        if let Some(end) = after_start.find("```") {
            after_start[..end].trim().to_string()
        } else {
            input.trim().to_string()
        }
    } else {
        input.trim().to_string()
    };
    // Strip xml-stylesheet processing instructions: they are browser-preview hints only
    // and must not be embedded in the .aipdf semantic payload (validate_xml rejects PIs).
    raw.lines()
        .filter(|l| !l.trim().starts_with("<?xml-stylesheet"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn wrap_document(blocks: Vec<String>) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<document version=\"1.0\" id=\"doc1\" lang=\"en\">\n{}\n</document>",
        blocks.join("\n")
    )
}

pub(crate) fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_extracts_rich_structure() {
        let md = "# Title\n\nIntro para with **bold** and `code`.\n\n\
- one\n- two\n\n\
1. first\n2. second\n\n\
| A | B |\n|---|---|\n| 1 | 2 |\n\n\
```rust\nfn main() {}\n```\n\n\
> a quotation\n\n\
![alt text](img.png)\n";
        let xml = markdown_to_xml(md);
        assert!(xml.contains(r#"<section id="s1" level="1"#));
        assert!(xml.contains("role=\"title\">Title<"));
        assert!(xml.contains("<paragraph"));
        assert!(xml.contains(r#"<list id="b"#) && xml.contains(r#"type="unordered""#));
        assert!(xml.contains(r#"type="ordered""#), "ordered list: {xml}");
        assert!(xml.contains("<table") && xml.contains("<cell header=\"true\">A<"));
        assert!(xml.contains(r#"<codeBlock id="b"# ) && xml.contains(r#"language="rust""#));
        assert!(xml.contains("fn main() {}"));
        assert!(xml.contains("<citation") && xml.contains("a quotation"));
        assert!(xml.contains("<figure") && xml.contains(r#"src="img.png""#));
        assert!(xml.contains(r#"alt="alt text""#));
        // emphasis is flattened, not literal markers
        assert!(xml.contains("bold and code"));
        crate::xml::validate_xml(&xml).expect("generated markdown XML must validate");
    }

    #[test]
    fn typst_extracts_rich_structure() {
        let typ = "= Title\n\n\
Intro with *bold* text.\n\n\
== Subsection\n\n\
- alpha\n- beta\n\n\
+ first\n+ second\n\n\
```python\nprint(1)\n```\n\n\
$ E = m c^2 $\n\n\
#figure(image(\"chart.png\"), caption: [A *chart*])\n";
        let xml = typst_to_xml(typ);
        assert!(xml.contains("role=\"title\">Title<"));
        assert!(xml.contains(r#"level="2""#), "subsection level: {xml}");
        assert!(xml.contains(r#"type="unordered""#) && xml.contains(">alpha<"));
        assert!(xml.contains(r#"type="ordered""#) && xml.contains(">first<"));
        assert!(xml.contains(r#"language="python""#) && xml.contains("print(1)"));
        assert!(xml.contains("<equation") && xml.contains("E = m c^2"));
        assert!(xml.contains("<figure") && xml.contains(r#"src="chart.png""#));
        assert!(xml.contains("A chart"), "caption flattened: {xml}");
        // bold markers stripped
        assert!(xml.contains("bold text") && !xml.contains("*bold*"));
        crate::xml::validate_xml(&xml).expect("generated typst XML must validate");
    }
}
