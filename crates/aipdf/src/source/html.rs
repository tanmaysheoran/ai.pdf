use super::xml_escape;

// ── HTML → semantic XML ──────────────────────────────────────────────────────

pub(crate) struct HtmlConverter {
    pub(crate) blocks: Vec<String>,
    pub(crate) section_id: usize,
    pub(crate) block_id: usize,
    pub(crate) section_open: bool,
}

impl HtmlConverter {
    pub(crate) fn new() -> Self {
        Self {
            blocks: Vec::new(),
            section_id: 1,
            block_id: 1,
            section_open: false,
        }
    }

    pub(crate) fn open_section(&mut self, level: usize, title: &str) {
        if self.section_open {
            self.blocks.push("</section>".to_string());
        }
        let sid = self.section_id;
        self.section_id += 1;
        let bid = self.block_id;
        self.block_id += 1;
        self.blocks.push(format!(
            r#"<section id="s{sid}" level="{level}" page="1">"#
        ));
        self.blocks.push(format!(
            r#"<title id="b{bid}" page="1" role="title">{}</title>"#,
            xml_escape(title)
        ));
        self.section_open = true;
    }

    pub(crate) fn ensure_section(&mut self) {
        if !self.section_open {
            let sid = self.section_id;
            self.section_id += 1;
            self.blocks
                .push(format!(r#"<section id="s{sid}" level="1" page="1">"#));
            self.section_open = true;
        }
    }

    pub(crate) fn push_paragraph(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.ensure_section();
        let bid = self.block_id;
        self.block_id += 1;
        self.blocks.push(format!(
            r#"<paragraph id="b{bid}" page="1" role="paragraph">{}</paragraph>"#,
            xml_escape(text)
        ));
    }

    pub(crate) fn push_list(&mut self, items: &[String], ordered: bool) {
        if items.is_empty() {
            return;
        }
        self.ensure_section();
        let bid = self.block_id;
        self.block_id += 1;
        let list_type = if ordered { "ordered" } else { "unordered" };
        let mut list = format!(r#"<list id="b{bid}" type="{list_type}">"#);
        for item in items {
            let ibid = self.block_id;
            self.block_id += 1;
            list.push_str(&format!(
                r#"<item id="b{ibid}">{}</item>"#,
                xml_escape(item.trim())
            ));
        }
        list.push_str("</list>");
        self.blocks.push(list);
    }

    pub(crate) fn push_table(&mut self, rows: &[Vec<(String, bool)>], caption: Option<&str>) {
        if rows.is_empty() {
            return;
        }
        self.ensure_section();
        let bid = self.block_id;
        self.block_id += 1;
        let mut tbl = format!(r#"<table id="b{bid}" page="1" role="table">"#);
        if let Some(cap) = caption {
            let cbid = self.block_id;
            self.block_id += 1;
            tbl.push_str(&format!(
                r#"<caption id="b{cbid}" role="caption">{}</caption>"#,
                xml_escape(cap.trim())
            ));
        }
        for row in rows {
            tbl.push_str("<row>");
            for (cell_text, is_header) in row {
                let header_attr = if *is_header { r#" header="true""# } else { "" };
                tbl.push_str(&format!(
                    "<cell{header_attr}>{}</cell>",
                    xml_escape(cell_text.trim())
                ));
            }
            tbl.push_str("</row>");
        }
        tbl.push_str("</table>");
        self.blocks.push(tbl);
    }

    pub(crate) fn push_code_block(&mut self, text: &str, language: Option<&str>) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.ensure_section();
        let bid = self.block_id;
        self.block_id += 1;
        let lang_attr = language
            .map(|l| format!(r#" language="{}""#, xml_escape(l)))
            .unwrap_or_default();
        self.blocks.push(format!(
            r#"<codeBlock id="b{bid}"{lang_attr}>{}</codeBlock>"#,
            xml_escape(text)
        ));
    }

    pub(crate) fn push_equation(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.ensure_section();
        let bid = self.block_id;
        self.block_id += 1;
        self.blocks.push(format!(
            r#"<equation id="b{bid}">{}</equation>"#,
            xml_escape(text)
        ));
    }

    pub(crate) fn push_citation(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.ensure_section();
        let bid = self.block_id;
        self.block_id += 1;
        self.blocks.push(format!(
            r#"<citation id="b{bid}" role="citation">{}</citation>"#,
            xml_escape(text)
        ));
    }

    pub(crate) fn push_figure(&mut self, src: &str, alt: &str, caption: Option<&str>) {
        self.ensure_section();
        let bid = self.block_id;
        self.block_id += 1;
        let mut fig = format!(r#"<figure id="b{bid}" page="1">"#);
        if !src.is_empty() || !alt.is_empty() {
            fig.push_str(&format!(
                r#"<image src="{}" alt="{}"/>"#,
                xml_escape(src),
                xml_escape(alt)
            ));
        }
        if let Some(cap) = caption {
            let cbid = self.block_id;
            self.block_id += 1;
            fig.push_str(&format!(
                r#"<caption id="b{cbid}">{}</caption>"#,
                xml_escape(cap.trim())
            ));
        }
        fig.push_str("</figure>");
        self.blocks.push(fig);
    }

    pub(crate) fn finish(mut self) -> Vec<String> {
        if self.section_open {
            self.blocks.push("</section>".to_string());
        }
        self.blocks
    }
}

pub(crate) fn heading_level(h: pulldown_cmark::HeadingLevel) -> usize {
    use pulldown_cmark::HeadingLevel;
    match h {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// A pending list being built (lists can nest; each level is flushed when its
/// `List` end tag arrives).
pub(crate) struct ListCtx {
    pub(crate) ordered: bool,
    pub(crate) items: Vec<String>,
}

/// A pending table being built.
pub(crate) struct TableCtx {
    pub(crate) rows: Vec<Vec<(String, bool)>>,
    pub(crate) cur: Vec<(String, bool)>,
    pub(crate) in_header: bool,
}
