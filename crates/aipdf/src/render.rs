use crate::source::xml_escape;
use quick_xml::events::Event;
use quick_xml::Reader;
use sha2::{Digest, Sha256};

const SEMANTIC_FILENAME: &str = "aipdf-semantic.xml.br";
const SEMANTIC_SUBTYPE: &str = "/application#aipdf+xml+br";

// ── Page geometry ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PageOptions {
    pub width: f32,
    pub height: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub margin_left: f32,
    pub margin_right: f32,
}

impl PageOptions {
    pub fn letter() -> Self {
        Self {
            width: 612.0,
            height: 792.0,
            margin_top: 72.0,
            margin_bottom: 72.0,
            margin_left: 72.0,
            margin_right: 72.0,
        }
    }

    pub fn a4() -> Self {
        Self {
            width: 595.0,
            height: 842.0,
            margin_top: 72.0,
            margin_bottom: 72.0,
            margin_left: 72.0,
            margin_right: 72.0,
        }
    }

    pub fn content_width(&self) -> f32 {
        self.width - self.margin_left - self.margin_right
    }
}

impl Default for PageOptions {
    fn default() -> Self {
        Self::letter()
    }
}

// ── Adobe Helvetica metrics (1/1000 pt) ──────────────────────────────────────

fn helvetica_width(c: char) -> f32 {
    match c {
        ' ' => 278.0,
        '!' => 278.0,
        '"' => 355.0,
        '#' => 556.0,
        '$' => 556.0,
        '%' => 889.0,
        '&' => 667.0,
        '\'' => 191.0,
        '(' => 333.0,
        ')' => 333.0,
        '*' => 389.0,
        '+' => 584.0,
        ',' => 278.0,
        '-' => 333.0,
        '.' => 278.0,
        '/' => 278.0,
        '0'..='9' => 556.0,
        ':' => 278.0,
        ';' => 278.0,
        '<' | '>' => 584.0,
        '=' => 584.0,
        '?' => 556.0,
        '@' => 1015.0,
        'A' => 667.0,
        'B' => 667.0,
        'C' => 722.0,
        'D' => 722.0,
        'E' => 667.0,
        'F' => 611.0,
        'G' => 778.0,
        'H' => 722.0,
        'I' => 278.0,
        'J' => 500.0,
        'K' => 667.0,
        'L' => 556.0,
        'M' => 833.0,
        'N' => 722.0,
        'O' => 778.0,
        'P' => 667.0,
        'Q' => 778.0,
        'R' => 722.0,
        'S' => 667.0,
        'T' => 611.0,
        'U' => 722.0,
        'V' => 667.0,
        'W' => 944.0,
        'X' => 667.0,
        'Y' => 667.0,
        'Z' => 611.0,
        '[' | ']' => 278.0,
        '\\' => 278.0,
        '^' => 469.0,
        '_' => 556.0,
        '`' => 333.0,
        'a' => 556.0,
        'b' => 556.0,
        'c' => 500.0,
        'd' => 556.0,
        'e' => 556.0,
        'f' => 278.0,
        'g' => 556.0,
        'h' => 556.0,
        'i' => 222.0,
        'j' => 222.0,
        'k' => 500.0,
        'l' => 222.0,
        'm' => 833.0,
        'n' => 556.0,
        'o' => 556.0,
        'p' => 556.0,
        'q' => 556.0,
        'r' => 333.0,
        's' => 500.0,
        't' => 278.0,
        'u' => 556.0,
        'v' => 500.0,
        'w' => 722.0,
        'x' => 500.0,
        'y' => 500.0,
        'z' => 500.0,
        '{' | '}' => 334.0,
        '|' => 260.0,
        '~' => 584.0,
        _ => 556.0,
    }
}

fn measure_width(text: &str, font_size: f32, bold: bool) -> f32 {
    if bold {
        text.chars()
            .map(|c| helvetica_width(c) * 1.05 * font_size / 1000.0)
            .sum()
    } else {
        text.chars()
            .map(|c| helvetica_width(c) * font_size / 1000.0)
            .sum()
    }
}

fn measure_mono_width(text: &str, font_size: f32) -> f32 {
    text.chars().count() as f32 * 600.0 * font_size / 1000.0
}

fn wrap_words(text: &str, font_size: f32, bold: bool, max_width: f32) -> Vec<String> {
    if text.trim().is_empty() {
        return vec![];
    }
    let space_w = 278.0 * if bold { 1.05 } else { 1.0 } * font_size / 1000.0;
    let mut lines = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0.0f32;

    for word in text.split_whitespace() {
        let w = measure_width(word, font_size, bold);
        if cur.is_empty() {
            cur = word.to_string();
            cur_w = w;
        } else if cur_w + space_w + w <= max_width + 0.5 {
            cur.push(' ');
            cur.push_str(word);
            cur_w += space_w + w;
        } else {
            lines.push(cur.clone());
            cur = word.to_string();
            cur_w = w;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

// ── Document element model ────────────────────────────────────────────────────

#[derive(Debug)]
enum DocElem {
    DocTitle(String),
    Heading { level: usize, text: String },
    Paragraph(String),
    Table { caption: Option<String>, rows: Vec<Vec<String>> },
    CodeBlock { language: Option<String>, text: String },
    List { ordered: bool, items: Vec<String> },
    Citation(String),
    Note(String),
    Figure { alt: String, src: String, caption: Option<String> },
}

fn parse_elements(xml: &str) -> Vec<DocElem> {
    let mut elems = Vec::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    // metadata title
    let mut in_meta_title = false;
    let mut in_meta = false;
    let mut meta_title = String::new();

    // section tracking
    let mut section_level = 1usize;

    // capture state
    let mut in_title = false;
    let mut in_para = false;
    let mut in_citation = false;
    let mut in_equation = false;
    let mut in_note = false;
    let mut _in_footnote = false;
    let mut in_item = false;
    let mut in_reference = false;
    let mut in_code = false;
    let mut code_lang: Option<String> = None;
    let mut current = String::new();

    // list state
    let mut in_list = false;
    let mut list_ordered = false;
    let mut list_items: Vec<String> = Vec::new();

    // table state
    let mut in_table = false;
    let mut table_caption: Option<String> = None;
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut in_row = false;
    let mut current_row: Vec<String> = Vec::new();
    let mut in_cell = false;
    let mut in_table_caption = false;

    // figure state
    let mut in_figure = false;
    let mut fig_src = String::new();
    let mut fig_alt = String::new();
    let mut fig_caption: Option<String> = None;
    let mut in_fig_caption = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "metadata" => in_meta = true,
                    "section" if !in_meta => {
                        section_level = attr_val(&e, b"level")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(1);
                    }
                    "title" if in_meta => {
                        in_meta_title = true;
                        meta_title.clear();
                    }
                    "title" if !in_meta && !in_table => {
                        in_title = true;
                        current.clear();
                    }
                    "paragraph" => {
                        in_para = true;
                        current.clear();
                    }
                    "citation" => {
                        in_citation = true;
                        current.clear();
                    }
                    "equation" => {
                        in_equation = true;
                        current.clear();
                    }
                    "note" | "footnote" if !in_table => {
                        in_note = true;
                        current.clear();
                    }
                    "codeBlock" => {
                        in_code = true;
                        code_lang = attr_val(&e, b"language");
                        current.clear();
                    }
                    "list" => {
                        in_list = true;
                        list_ordered = attr_val(&e, b"type")
                            .map(|t| t == "ordered")
                            .unwrap_or(false);
                        list_items.clear();
                    }
                    "item" | "reference" if in_list => {
                        in_item = true;
                        current.clear();
                    }
                    "references" => {
                        in_list = true;
                        list_ordered = false;
                        list_items.clear();
                    }
                    "reference" if !in_list => {
                        in_reference = true;
                        current.clear();
                    }
                    "table" if !in_meta => {
                        in_table = true;
                        table_caption = None;
                        table_rows.clear();
                    }
                    "caption" if in_table => {
                        in_table_caption = true;
                        current.clear();
                    }
                    "row" if in_table => {
                        in_row = true;
                        current_row.clear();
                    }
                    "cell" if in_row => {
                        in_cell = true;
                        current.clear();
                    }
                    "figure" => {
                        in_figure = true;
                        fig_src.clear();
                        fig_alt.clear();
                        fig_caption = None;
                    }
                    "caption" if in_figure => {
                        in_fig_caption = true;
                        current.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "image" && in_figure {
                    fig_src = attr_val(&e, b"src").unwrap_or_default();
                    fig_alt = attr_val(&e, b"alt").unwrap_or_default();
                }
            }
            Ok(Event::Text(t)) => {
                let text = t.unescape().unwrap_or_default().to_string();
                if in_meta_title {
                    meta_title.push_str(&text);
                } else if in_cell || in_item || in_para || in_title || in_citation
                    || in_equation || in_note || in_code || in_reference
                    || in_table_caption || in_fig_caption
                {
                    current.push_str(text.trim());
                }
            }
            Ok(Event::CData(t)) => {
                if in_code || in_equation {
                    current.push_str(String::from_utf8_lossy(&t).trim());
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "metadata" => {
                        in_meta = false;
                        if !meta_title.trim().is_empty() {
                            elems.push(DocElem::DocTitle(meta_title.trim().to_string()));
                        }
                    }
                    "title" if in_meta_title => {
                        in_meta_title = false;
                    }
                    "title" if in_title => {
                        elems.push(DocElem::Heading {
                            level: section_level,
                            text: current.trim().to_string(),
                        });
                        in_title = false;
                    }
                    "paragraph" if in_para => {
                        if !current.trim().is_empty() {
                            elems.push(DocElem::Paragraph(current.trim().to_string()));
                        }
                        in_para = false;
                    }
                    "citation" if in_citation => {
                        elems.push(DocElem::Citation(current.trim().to_string()));
                        in_citation = false;
                    }
                    "equation" if in_equation => {
                        elems.push(DocElem::Paragraph(format!(
                            "[ {} ]",
                            current.trim()
                        )));
                        in_equation = false;
                    }
                    "note" | "footnote" if in_note => {
                        elems.push(DocElem::Note(current.trim().to_string()));
                        in_note = false;
                        _in_footnote = false;
                    }
                    "codeBlock" if in_code => {
                        elems.push(DocElem::CodeBlock {
                            language: code_lang.take(),
                            text: current.trim().to_string(),
                        });
                        in_code = false;
                    }
                    "item" | "reference" if in_item => {
                        list_items.push(current.trim().to_string());
                        in_item = false;
                    }
                    "list" | "references" if in_list => {
                        if !list_items.is_empty() {
                            elems.push(DocElem::List {
                                ordered: list_ordered,
                                items: list_items.clone(),
                            });
                        }
                        in_list = false;
                    }
                    "reference" if in_reference => {
                        if !current.trim().is_empty() {
                            elems.push(DocElem::Paragraph(current.trim().to_string()));
                        }
                        in_reference = false;
                    }
                    "caption" if in_table_caption => {
                        table_caption = Some(current.trim().to_string());
                        in_table_caption = false;
                    }
                    "cell" if in_cell => {
                        current_row.push(current.trim().to_string());
                        in_cell = false;
                    }
                    "row" if in_row => {
                        if !current_row.is_empty() {
                            table_rows.push(current_row.clone());
                        }
                        in_row = false;
                    }
                    "table" if in_table => {
                        if !table_rows.is_empty() {
                            elems.push(DocElem::Table {
                                caption: table_caption.take(),
                                rows: table_rows.clone(),
                            });
                        }
                        in_table = false;
                    }
                    "caption" if in_fig_caption => {
                        fig_caption = Some(current.trim().to_string());
                        in_fig_caption = false;
                    }
                    "figure" if in_figure => {
                        elems.push(DocElem::Figure {
                            alt: fig_alt.clone(),
                            src: fig_src.clone(),
                            caption: fig_caption.take(),
                        });
                        in_figure = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
    }
    elems
}

fn attr_val(e: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .map(|a| String::from_utf8_lossy(a.value.as_ref()).to_string())
}

// ── Layout engine ─────────────────────────────────────────────────────────────

struct Layout {
    opts: PageOptions,
    pages: Vec<String>,       // completed page content streams
    current: String,          // current page content stream
    cursor_y: f32,
    page_num: usize,
}

const BODY_SIZE: f32 = 11.0;
const BODY_LEAD: f32 = 15.4; // 11 × 1.4
const CODE_SIZE: f32 = 9.0;
const CODE_LEAD: f32 = 13.0;
const PARA_SPACE: f32 = 8.0;
const SECTION_SPACE: f32 = 16.0;
const FOOTER_H: f32 = 20.0;

impl Layout {
    fn new(opts: PageOptions) -> Self {
        let top = opts.height - opts.margin_top;
        let mut this = Self {
            opts,
            pages: Vec::new(),
            current: String::new(),
            cursor_y: top,
            page_num: 1,
        };
        this.begin_page();
        this
    }

    fn begin_page(&mut self) {
        self.current.clear();
    }

    fn finish_page(&mut self) {
        // Footer: "Page N" centered
        let footer_text = format!("Page {}", self.page_num);
        let fw = measure_width(&footer_text, 9.0, false);
        let fx = self.opts.margin_left + (self.opts.content_width() - fw) / 2.0;
        let fy = self.opts.margin_bottom / 2.0;
        self.current.push_str(&format!(
            "BT\n/F1 9 Tf\n{} {} Td\n({}) Tj\nET\n",
            fx,
            fy,
            pdf_str(&footer_text)
        ));
        self.pages.push(self.current.clone());
        self.current.clear();
        self.cursor_y = self.opts.height - self.opts.margin_top;
        self.page_num += 1;
    }

    fn available_y(&self) -> f32 {
        self.cursor_y - self.opts.margin_bottom - FOOTER_H
    }

    fn ensure_space(&mut self, needed: f32) {
        if self.available_y() < needed {
            self.finish_page();
            self.begin_page();
        }
    }

    fn draw_text_lines(
        &mut self,
        lines: &[String],
        x: f32,
        font: &str,
        size: f32,
        leading: f32,
    ) {
        if lines.is_empty() {
            return;
        }
        self.current.push_str(&format!(
            "BT\n/{font} {size:.1} Tf\n{} {} Td\n{} TL\n",
            x,
            self.cursor_y,
            leading
        ));
        for line in lines {
            self.current
                .push_str(&format!("({}) Tj\nT*\n", pdf_str(line)));
        }
        self.current.push_str("ET\n");
        self.cursor_y -= lines.len() as f32 * leading;
    }

    fn add_space(&mut self, pts: f32) {
        self.cursor_y -= pts;
    }

    // ── Renderers ────────────────────────────────────────────────────────────

    fn render_doc_title(&mut self, text: &str) {
        let size = 22.0;
        let leading = 28.0;
        let lines = wrap_words(text, size, true, self.opts.content_width());
        let needed = lines.len() as f32 * leading + SECTION_SPACE * 2.0;
        self.ensure_space(needed);
        // Center the title
        let x = self.opts.margin_left;
        self.current.push_str(&format!(
            "BT\n/F2 {size:.1} Tf\n{x} {} Td\n{leading} TL\n",
            self.cursor_y
        ));
        for line in &lines {
            let w = measure_width(line, size, true);
            let offset = ((self.opts.content_width() - w) / 2.0).max(0.0);
            self.current.push_str(&format!(
                "({}) Tj\nT*\n",
                pdf_str(&format!("{}{}", " ".repeat((offset / (size * 0.278)) as usize), line))
            ));
        }
        self.current.push_str("ET\n");
        self.cursor_y -= lines.len() as f32 * leading;
        self.add_space(SECTION_SPACE);
        // Horizontal rule
        self.draw_hline(0.5, 0.3);
        self.add_space(SECTION_SPACE);
    }

    fn render_heading(&mut self, level: usize, text: &str) {
        let (size, leading, space_before) = match level {
            1 => (18.0f32, 24.0f32, 20.0f32),
            2 => (14.0, 19.0, 14.0),
            3 => (12.0, 17.0, 10.0),
            _ => (11.0, 15.4, 8.0),
        };
        let lines = wrap_words(text, size, true, self.opts.content_width());
        let needed = lines.len() as f32 * leading + space_before + 6.0;
        self.ensure_space(needed);
        self.add_space(space_before);
        self.draw_text_lines(&lines, self.opts.margin_left, "F2", size, leading);
        self.add_space(6.0);
        if level <= 2 {
            self.draw_hline(0.5, 0.6);
            self.add_space(4.0);
        }
    }

    fn render_paragraph(&mut self, text: &str) {
        let lines = wrap_words(text, BODY_SIZE, false, self.opts.content_width());
        if lines.is_empty() {
            return;
        }
        let needed = lines.len() as f32 * BODY_LEAD + PARA_SPACE;
        self.ensure_space(needed);
        self.draw_text_lines(&lines, self.opts.margin_left, "F1", BODY_SIZE, BODY_LEAD);
        self.add_space(PARA_SPACE);
    }

    fn render_citation(&mut self, text: &str) {
        let indent = 20.0;
        let x = self.opts.margin_left + indent;
        let w = self.opts.content_width() - indent;
        let lines = wrap_words(text, BODY_SIZE, false, w);
        if lines.is_empty() {
            return;
        }
        let block_h = lines.len() as f32 * BODY_LEAD;
        let needed = block_h + PARA_SPACE * 2.0;
        self.ensure_space(needed);
        self.add_space(PARA_SPACE / 2.0);
        // Left vertical bar
        let bar_x = self.opts.margin_left + 4.0;
        let bar_top = self.cursor_y + BODY_SIZE * 0.2;
        let bar_bot = self.cursor_y - block_h;
        self.current.push_str(&format!(
            "q 0.4 G 2 w {bar_x} {bar_top} m {bar_x} {bar_bot} l S Q\n"
        ));
        self.draw_text_lines(&lines, x, "F1", BODY_SIZE, BODY_LEAD);
        self.add_space(PARA_SPACE);
    }

    fn render_note(&mut self, text: &str) {
        let prefixed = format!("Note: {text}");
        let lines = wrap_words(&prefixed, BODY_SIZE, false, self.opts.content_width() - 10.0);
        if lines.is_empty() {
            return;
        }
        let block_h = lines.len() as f32 * BODY_LEAD + 8.0;
        self.ensure_space(block_h + PARA_SPACE);
        let bx = self.opts.margin_left;
        let by = self.cursor_y - block_h + 4.0;
        // Light gray background
        self.current.push_str(&format!(
            "q 0.93 g {bx} {by} {} {block_h} re f Q\n",
            self.opts.content_width()
        ));
        self.add_space(4.0);
        self.draw_text_lines(&lines, bx + 6.0, "F1", BODY_SIZE, BODY_LEAD);
        self.add_space(PARA_SPACE);
    }

    fn render_code_block(&mut self, text: &str, _language: Option<&str>) {
        // Split into lines, word-wrap long lines at content width
        let raw_lines: Vec<&str> = text.lines().collect();
        // Soft-wrap each raw line at code content width
        let code_width = self.opts.content_width() - 16.0;
        let mut wrapped: Vec<String> = Vec::new();
        for raw in &raw_lines {
            if raw.is_empty() {
                wrapped.push(String::new());
            } else if measure_mono_width(raw, CODE_SIZE) <= code_width {
                wrapped.push(raw.to_string());
            } else {
                // Split at character boundary
                let chars_per_line =
                    (code_width / (600.0 * CODE_SIZE / 1000.0)).floor() as usize;
                let chars_per_line = chars_per_line.max(1);
                let mut rest = raw.chars().collect::<Vec<_>>();
                while !rest.is_empty() {
                    let chunk: String = rest.drain(..rest.len().min(chars_per_line)).collect();
                    wrapped.push(chunk);
                }
            }
        }

        let block_h = wrapped.len() as f32 * CODE_LEAD + 12.0;
        self.ensure_space(block_h + PARA_SPACE);
        let bx = self.opts.margin_left;
        let by = self.cursor_y - block_h + 4.0;
        // Gray background box
        self.current.push_str(&format!(
            "q 0.92 g {bx} {by} {} {block_h} re f 0.75 G 0.5 w {bx} {by} {} {block_h} re S Q\n",
            self.opts.content_width(),
            self.opts.content_width()
        ));
        self.add_space(6.0);
        if !wrapped.is_empty() {
            self.current.push_str(&format!(
                "BT\n/F3 {CODE_SIZE:.1} Tf\n{} {} Td\n{CODE_LEAD} TL\n",
                bx + 8.0,
                self.cursor_y
            ));
            for line in &wrapped {
                self.current
                    .push_str(&format!("({}) Tj\nT*\n", pdf_str(line)));
            }
            self.current.push_str("ET\n");
            self.cursor_y -= wrapped.len() as f32 * CODE_LEAD;
        }
        self.add_space(6.0 + PARA_SPACE);
    }

    fn render_list(&mut self, ordered: bool, items: &[String]) {
        let indent = 20.0;
        let bullet_x = self.opts.margin_left + 4.0;
        let text_x = self.opts.margin_left + indent;
        let text_w = self.opts.content_width() - indent;

        let all_lines: Vec<(String, Vec<String>)> = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let bullet = if ordered {
                    format!("{}.", i + 1)
                } else {
                    "\u{2022}".to_string() // bullet char
                };
                let wrapped = wrap_words(item, BODY_SIZE, false, text_w);
                (bullet, wrapped)
            })
            .collect();

        let total_h: f32 = all_lines
            .iter()
            .map(|(_, lines)| lines.len() as f32 * BODY_LEAD)
            .sum::<f32>()
            + PARA_SPACE;

        self.ensure_space(total_h);

        for (bullet, lines) in &all_lines {
            if lines.is_empty() {
                continue;
            }
            // bullet marker
            self.current.push_str(&format!(
                "BT\n/F1 {BODY_SIZE:.1} Tf\n{bullet_x} {} Td\n({}) Tj\nET\n",
                self.cursor_y,
                pdf_str(bullet)
            ));
            self.draw_text_lines(lines, text_x, "F1", BODY_SIZE, BODY_LEAD);
        }
        self.add_space(PARA_SPACE);
    }

    fn render_table(&mut self, caption: Option<&str>, rows: &[Vec<String>]) {
        if rows.is_empty() {
            return;
        }
        let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(1);
        let col_w = self.opts.content_width() / ncols as f32;
        let row_h = BODY_LEAD + 6.0;
        let total_h = rows.len() as f32 * row_h
            + caption.map(|_| BODY_LEAD + PARA_SPACE).unwrap_or(0.0)
            + PARA_SPACE;

        if total_h > self.available_y() * 0.9 {
            // Table too big for current page — push to next
            self.finish_page();
            self.begin_page();
        }

        if let Some(cap) = caption {
            let lines = wrap_words(cap, BODY_SIZE - 1.0, true, self.opts.content_width());
            self.draw_text_lines(&lines, self.opts.margin_left, "F2", BODY_SIZE - 1.0, BODY_LEAD);
            self.add_space(PARA_SPACE / 2.0);
        }

        let tbl_x = self.opts.margin_left;

        for (ri, row) in rows.iter().enumerate() {
            let is_header = ri == 0;
            let y_top = self.cursor_y;
            let y_bot = y_top - row_h;

            // Header fill
            if is_header {
                self.current.push_str(&format!(
                    "q 0.85 g {tbl_x} {y_bot} {} {row_h} re f Q\n",
                    self.opts.content_width()
                ));
            }

            // Cell text + vertical dividers
            for (ci, cell) in row.iter().enumerate() {
                let cx = tbl_x + ci as f32 * col_w;
                let text_x = cx + 4.0;
                let available_w = col_w - 8.0;
                let lines = wrap_words(cell, BODY_SIZE, is_header, available_w);
                let font = if is_header { "F2" } else { "F1" };
                // Single line render per cell (truncate if overflow)
                let text = lines.first().map(String::as_str).unwrap_or("");
                let text_y = y_top - BODY_SIZE - 3.0;
                self.current.push_str(&format!(
                    "BT\n/{font} {BODY_SIZE:.1} Tf\n{text_x} {text_y} Td\n({}) Tj\nET\n",
                    pdf_str(text)
                ));
                // Vertical line after cell (except last)
                if ci < ncols - 1 {
                    let vx = cx + col_w;
                    self.current.push_str(&format!(
                        "q 0.5 G 0.5 w {vx} {y_top} m {vx} {y_bot} l S Q\n"
                    ));
                }
            }

            // Horizontal rule between rows
            self.current.push_str(&format!(
                "q 0.4 G 0.5 w {tbl_x} {y_bot} m {} {y_bot} l S Q\n",
                tbl_x + self.opts.content_width()
            ));

            self.cursor_y -= row_h;

            // Check if we need a new page mid-table (for very long tables)
            if ri < rows.len() - 1 && self.available_y() < row_h * 2.0 {
                // Draw left/right borders for what we have, then new page
                self.finish_page();
                self.begin_page();
            }
        }

        // Outer border
        let tbl_top = self.cursor_y + rows.len() as f32 * row_h;
        let tbl_bot = self.cursor_y;
        let _tbl_right = tbl_x + self.opts.content_width();
        self.current.push_str(&format!(
            "q 0.4 G 0.5 w {tbl_x} {tbl_bot} {w} {h} re S Q\n",
            w = self.opts.content_width(),
            h = tbl_top - tbl_bot
        ));

        self.add_space(PARA_SPACE);
    }

    fn render_figure(&mut self, alt: &str, src: &str, caption: Option<&str>) {
        // Render a placeholder box with alt text
        let box_h = 80.0;
        let needed = box_h + PARA_SPACE * 2.0 + caption.map(|_| BODY_LEAD + 4.0).unwrap_or(0.0);
        self.ensure_space(needed);
        let bx = self.opts.margin_left;
        let by = self.cursor_y - box_h;
        let bw = self.opts.content_width();
        self.current.push_str(&format!(
            "q 0.9 g {bx} {by} {bw} {box_h} re f 0.6 G 1 w {bx} {by} {bw} {box_h} re S Q\n"
        ));
        let label = if !alt.is_empty() {
            alt.to_string()
        } else if !src.is_empty() {
            format!("[Figure: {src}]")
        } else {
            "[Figure]".to_string()
        };
        let label_w = measure_width(&label, BODY_SIZE, false);
        let lx = bx + (bw - label_w) / 2.0;
        let ly = by + box_h / 2.0 - BODY_SIZE / 2.0;
        self.current.push_str(&format!(
            "BT\n/F1 {BODY_SIZE:.1} Tf\n{lx} {ly} Td\n({}) Tj\nET\n",
            pdf_str(&label)
        ));
        self.cursor_y -= box_h;
        if let Some(cap) = caption {
            self.add_space(4.0);
            let cap_lines = wrap_words(cap, BODY_SIZE - 1.0, false, bw);
            for line in &cap_lines {
                let lw = measure_width(line, BODY_SIZE - 1.0, false);
                let lx = bx + (bw - lw) / 2.0;
                self.current.push_str(&format!(
                    "BT\n/F1 {:.1} Tf\n{lx} {} Td\n({}) Tj\nET\n",
                    BODY_SIZE - 1.0,
                    self.cursor_y,
                    pdf_str(line)
                ));
                self.cursor_y -= BODY_LEAD;
            }
        }
        self.add_space(PARA_SPACE);
    }

    fn draw_hline(&mut self, gray: f32, width: f32) {
        let x0 = self.opts.margin_left;
        let x1 = x0 + self.opts.content_width();
        self.current.push_str(&format!(
            "q {gray} G {width} w {x0} {} m {x1} {} l S Q\n",
            self.cursor_y, self.cursor_y
        ));
        self.cursor_y -= 1.0;
    }

    fn finalize(mut self) -> (Vec<String>, usize) {
        // close the last page
        self.finish_page();
        let total = self.pages.len();
        (self.pages, total)
    }
}

// ── PDF assembly ──────────────────────────────────────────────────────────────

struct Assembler {
    objects: Vec<Vec<u8>>,
}

impl Assembler {
    fn new() -> Self {
        Self { objects: Vec::new() }
    }

    fn add(&mut self, bytes: Vec<u8>) -> usize {
        let id = self.objects.len() + 1;
        self.objects.push(bytes);
        id
    }

    fn reserve(&mut self) -> usize {
        self.add(Vec::new())
    }

    fn set(&mut self, id: usize, bytes: Vec<u8>) {
        self.objects[id - 1] = bytes;
    }

    fn build(self, root_id: usize, title: &str) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");
        let mut offsets = vec![0usize];
        for (i, obj) in self.objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            out.extend_from_slice(obj);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref_offset = out.len();
        let total = self.objects.len() + 1;
        out.extend_from_slice(format!("xref\n0 {total}\n").as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for o in offsets.iter().skip(1) {
            out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        let esc_title = pdf_str(title);
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {total} /Root {root_id} 0 R /Info << /Title ({esc_title}) /Producer (aipdf) >> >>\nstartxref\n{xref_offset}\n%%EOF\n"
            )
            .as_bytes(),
        );
        out
    }
}

fn stream_obj(bytes: &[u8], dict: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let dict = dict.trim_end_matches(">>").trim();
    out.extend_from_slice(format!("{dict} /Length {} >>\nstream\n", bytes.len()).as_bytes());
    out.extend_from_slice(bytes);
    out.extend_from_slice(b"\nendstream");
    out
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn xmp_metadata(title: &str, xml_bytes: usize, compressed_bytes: usize) -> String {
    format!(
        r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about=""
      xmlns:dc="http://purl.org/dc/elements/1.1/"
      xmlns:aipdf="https://aipdf.org/ns/1.0/">
      <dc:title><rdf:Alt><rdf:li xml:lang="x-default">{}</rdf:li></rdf:Alt></dc:title>
      <aipdf:Version>1.0</aipdf:Version>
      <aipdf:SemanticFile>{SEMANTIC_FILENAME}</aipdf:SemanticFile>
      <aipdf:SemanticEncoding>brotli</aipdf:SemanticEncoding>
      <aipdf:SemanticLayerPresent>true</aipdf:SemanticLayerPresent>
      <aipdf:SemanticXmlBytes>{xml_bytes}</aipdf:SemanticXmlBytes>
      <aipdf:SemanticCompressedBytes>{compressed_bytes}</aipdf:SemanticCompressedBytes>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#,
        xml_escape(title)
    )
}

// ── Public entry point ────────────────────────────────────────────────────────

pub fn build_rendered_pdf(
    xml: &str,
    compressed: &[u8],
    title: &str,
    page_opts: &PageOptions,
) -> Vec<u8> {
    let elems = parse_elements(xml);
    let mut layout = Layout::new(page_opts.clone());

    for elem in elems {
        match elem {
            DocElem::DocTitle(t) => layout.render_doc_title(&t),
            DocElem::Heading { level, text } => layout.render_heading(level, &text),
            DocElem::Paragraph(t) => layout.render_paragraph(&t),
            DocElem::Citation(t) => layout.render_citation(&t),
            DocElem::Note(t) => layout.render_note(&t),
            DocElem::CodeBlock { language, text } => {
                layout.render_code_block(&text, language.as_deref())
            }
            DocElem::List { ordered, items } => layout.render_list(ordered, &items),
            DocElem::Table { caption, rows } => {
                layout.render_table(caption.as_deref(), &rows)
            }
            DocElem::Figure { alt, src, caption } => {
                layout.render_figure(&alt, &src, caption.as_deref())
            }
        }
    }

    let (page_streams, page_count) = layout.finalize();

    // Build PDF object tree
    let mut asm = Assembler::new();

    // Reserve slots for objects whose IDs we need to know in advance
    let catalog_id = asm.reserve(); // 1
    let pages_id = asm.reserve(); // 2

    // Add font objects
    let f1_id = asm.add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec());
    let f2_id =
        asm.add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>".to_vec());
    let f3_id = asm.add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>".to_vec());

    let font_res = format!(
        "<< /F1 {f1_id} 0 R /F2 {f2_id} 0 R /F3 {f3_id} 0 R >>"
    );

    // Add page content streams and page objects
    let mut page_ids = Vec::new();
    for stream in &page_streams {
        let cs_id = asm.add(stream_obj(stream.as_bytes(), "<< >>"));
        let pg_id = asm.add(
            format!(
                "<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 {} {}] /Resources << /Font {font_res} >> /Contents {cs_id} 0 R >>",
                page_opts.width, page_opts.height
            )
            .into_bytes(),
        );
        page_ids.push(pg_id);
    }

    // XMP metadata
    let xmp = xmp_metadata(title, xml.len(), compressed.len());
    let xmp_id = asm.add(stream_obj(
        xmp.as_bytes(),
        "<< /Type /Metadata /Subtype /XML >>",
    ));

    // Semantic layer
    let checksum = hex_sha256(compressed);
    let esc_title = pdf_str(title);
    let ef_dict = format!(
        "<< /Type /EmbeddedFile /Subtype {SEMANTIC_SUBTYPE} /Params << /Size {} /CheckSum <{checksum}> >> >>",
        xml.len()
    );
    let ef_id = asm.add(stream_obj(compressed, &ef_dict));
    let filespec_id = asm.add(
        format!(
            "<< /Type /Filespec /F ({SEMANTIC_FILENAME}) /UF ({SEMANTIC_FILENAME}) /Desc ({esc_title} semantic XML) /AFRelationship /Data /EF << /F {ef_id} 0 R /UF {ef_id} 0 R >> >>"
        )
        .into_bytes(),
    );
    let names_id = asm.add(
        format!("<< /Names [(aipdf-semantic.xml.br) {filespec_id} 0 R] >>").into_bytes(),
    );

    // Fill in Pages object
    let kids = page_ids
        .iter()
        .map(|id| format!("{id} 0 R"))
        .collect::<Vec<_>>()
        .join(" ");
    asm.set(
        pages_id,
        format!(
            "<< /Type /Pages /Kids [{kids}] /Count {page_count} >>"
        )
        .into_bytes(),
    );

    // Fill in Catalog
    asm.set(
        catalog_id,
        format!(
            "<< /Type /Catalog /Pages {pages_id} 0 R /Metadata {xmp_id} 0 R /Names << /EmbeddedFiles {names_id} 0 R >> /AF [{filespec_id} 0 R] >>"
        )
        .into_bytes(),
    );

    asm.build(catalog_id, title)
}

// ── PDF string encoding ───────────────────────────────────────────────────────

fn pdf_str(input: &str) -> String {
    let mut out = String::new();
    for c in input.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\r' | '\n' => out.push(' '),
            // Keep printable ASCII, replace others with '?'
            c if (c as u32) < 32 => out.push(' '),
            c if (c as u32) > 126 => out.push('?'),
            c => out.push(c),
        }
    }
    out
}
