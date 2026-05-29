use super::{MarkdownNode};

pub(super) fn heading_node(depth: usize, value: &str) -> MarkdownNode {
    MarkdownNode {
        node_type: "heading",
        url: None,
        alt: None,
        value: None,
        depth: Some(depth.clamp(1, 6)),
        lang: None,
        ordered: None,
        children: vec![text_node(value)],
    }
}

pub(super) fn paragraph_node(value: &str) -> MarkdownNode {
    MarkdownNode {
        node_type: "paragraph",
        url: None,
        alt: None,
        value: None,
        depth: None,
        lang: None,
        ordered: None,
        children: vec![text_node(value)],
    }
}

/// An MDAST `image` node wrapped in a `paragraph` — images are phrasing content
/// and may not sit at the document root, so they live inside a block.
pub(super) fn image_paragraph_node(src: &str, alt: &str) -> MarkdownNode {
    let image = MarkdownNode {
        node_type: "image",
        value: None,
        depth: None,
        lang: None,
        ordered: None,
        url: Some(src.to_string()),
        alt: Some(alt.to_string()),
        children: Vec::new(),
    };
    MarkdownNode {
        node_type: "paragraph",
        value: None,
        depth: None,
        lang: None,
        ordered: None,
        url: None,
        alt: None,
        children: vec![image],
    }
}

pub(super) fn blockquote_node(value: &str) -> MarkdownNode {
    MarkdownNode {
        node_type: "blockquote",
        url: None,
        alt: None,
        value: None,
        depth: None,
        lang: None,
        ordered: None,
        children: vec![paragraph_node(value)],
    }
}

pub(super) fn list_item_node(value: &str) -> MarkdownNode {
    MarkdownNode {
        node_type: "listItem",
        url: None,
        alt: None,
        value: None,
        depth: None,
        lang: None,
        ordered: None,
        children: vec![paragraph_node(value)],
    }
}

pub(super) fn table_node(rows: &[Vec<String>]) -> MarkdownNode {
    MarkdownNode {
        node_type: "table",
        url: None,
        alt: None,
        value: None,
        depth: None,
        lang: None,
        ordered: None,
        children: rows
            .iter()
            .map(|row| MarkdownNode {
                node_type: "tableRow",
                url: None,
                alt: None,
                value: None,
                depth: None,
                lang: None,
                ordered: None,
                children: row
                    .iter()
                    .map(|cell| MarkdownNode {
                        node_type: "tableCell",
                        url: None,
                        alt: None,
                        value: None,
                        depth: None,
                        lang: None,
                        ordered: None,
                        children: vec![text_node(cell)],
                    })
                    .collect(),
            })
            .collect(),
    }
}

pub(super) fn value_node(node_type: &'static str, value: &str, lang: Option<String>) -> MarkdownNode {
    MarkdownNode {
        node_type,
        value: Some(value.to_string()),
        depth: None,
        lang,
        ordered: None,
        url: None,
        alt: None,
        children: Vec::new(),
    }
}

pub(super) fn text_node(value: &str) -> MarkdownNode {
    value_node("text", value, None)
}
