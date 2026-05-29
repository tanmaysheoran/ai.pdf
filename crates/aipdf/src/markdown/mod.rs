use serde::Serialize;

mod ast;
mod ast_nodes;
mod render;

pub use render::xml_to_markdown;
pub use ast::xml_to_markdown_ast_json;

#[derive(Debug, Serialize)]
pub(super) struct MarkdownAst {
    #[serde(rename = "type")]
    pub(super) node_type: &'static str,
    pub(super) children: Vec<MarkdownNode>,
}

#[derive(Debug, Serialize, Clone)]
pub(super) struct MarkdownNode {
    #[serde(rename = "type")]
    pub(super) node_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) depth: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) lang: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) ordered: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) alt: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(super) children: Vec<MarkdownNode>,
}

pub(super) fn attr_value(e: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .map(|a| String::from_utf8_lossy(a.value.as_ref()).to_string())
}
