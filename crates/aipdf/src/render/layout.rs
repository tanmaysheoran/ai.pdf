use super::image::ImageObj;
use super::options::{PageOptions, wrap_words};
use super::parse::BlockCoord;
use crate::font::{Font, GlyphSet};
use std::path::PathBuf;

// ── Layout engine ─────────────────────────────────────────────────────────────

pub(super) struct Layout {
    pub(super) opts: PageOptions,
    pub(super) font: Font,
    pub(super) glyphs: GlyphSet,
    pub(super) base_dir: Option<PathBuf>,
    pub(super) images: Vec<ImageObj>,
    pub(super) coords: Vec<BlockCoord>,
    pub(super) pages: Vec<String>, // completed page content streams
    pub(super) current: String,    // current page content stream
    pub(super) cursor_y: f32,
    pub(super) page_num: usize,
}

pub(super) const BODY_SIZE: f32 = 11.0;
pub(super) const BODY_LEAD: f32 = 15.4; // 11 × 1.4
pub(super) const CODE_SIZE: f32 = 9.0;
pub(super) const CODE_LEAD: f32 = 13.0;
pub(super) const PARA_SPACE: f32 = 8.0;
pub(super) const SECTION_SPACE: f32 = 16.0;
pub(super) const FOOTER_H: f32 = 20.0;

impl Layout {
    pub(super) fn new(opts: PageOptions, font: Font, base_dir: Option<PathBuf>) -> Self {
        let top = opts.height - opts.margin_top;
        let mut this = Self {
            opts,
            font,
            glyphs: GlyphSet::new(),
            base_dir,
            images: Vec::new(),
            coords: Vec::new(),
            pages: Vec::new(),
            current: String::new(),
            cursor_y: top,
            page_num: 1,
        };
        this.begin_page();
        this
    }

    pub(super) fn begin_page(&mut self) {
        self.current.clear();
    }

    pub(super) fn measure(&self, text: &str, size: f32) -> f32 {
        self.font.text_width(text, size)
    }

    pub(super) fn wrap(&self, text: &str, size: f32, max_width: f32) -> Vec<String> {
        wrap_words(&self.font, text, size, max_width)
    }

    /// Encode one line to a content-stream hex GID string, recording glyphs.
    pub(super) fn hex(&mut self, line: &str) -> String {
        self.glyphs.encode_hex(&self.font, line)
    }

    pub(super) fn finish_page(&mut self) {
        // Footer: "Page N" centered
        let footer_text = format!("Page {}", self.page_num);
        let fw = self.measure(&footer_text, 9.0);
        let fx = self.opts.margin_left + (self.opts.content_width() - fw) / 2.0;
        let fy = self.opts.margin_bottom / 2.0;
        let h = self.hex(&footer_text);
        self.current.push_str(&format!(
            "BT\n/F1 9 Tf\n{fx:.2} {fy:.2} Td\n<{h}> Tj\nET\n"
        ));
        self.pages.push(self.current.clone());
        self.current.clear();
        self.cursor_y = self.opts.height - self.opts.margin_top;
        self.page_num += 1;
    }

    pub(super) fn available_y(&self) -> f32 {
        self.cursor_y - self.opts.margin_bottom - FOOTER_H
    }

    pub(super) fn ensure_space(&mut self, needed: f32) {
        if self.available_y() < needed {
            self.finish_page();
            self.begin_page();
        }
    }

    /// Draw word-wrapped lines starting at the cursor, advancing it downward.
    /// `bold` synthesizes a heavier weight from the single embedded face via the
    /// fill+stroke text render mode, isolated inside a q/Q so it cannot leak.
    pub(super) fn draw_text_lines(&mut self, lines: &[String], x: f32, size: f32, leading: f32, bold: bool) {
        if lines.is_empty() {
            return;
        }
        if bold {
            self.current
                .push_str(&format!("q 2 Tr {:.2} w 0 G\n", size * 0.03));
        }
        self.current.push_str(&format!(
            "BT\n/F1 {size:.1} Tf\n{x:.2} {:.2} Td\n{leading:.1} TL\n",
            self.cursor_y
        ));
        for line in lines {
            let h = self.hex(line);
            self.current.push_str(&format!("<{h}> Tj\nT*\n"));
        }
        self.current.push_str("ET\n");
        if bold {
            self.current.push_str("Q\n");
        }
        self.cursor_y -= lines.len() as f32 * leading;
    }

    /// Draw a single line at an absolute (x, y) without moving the cursor.
    pub(super) fn draw_single(&mut self, text: &str, x: f32, y: f32, size: f32, bold: bool) {
        let h = self.hex(text);
        if bold {
            self.current
                .push_str(&format!("q 2 Tr {:.2} w 0 G\n", size * 0.03));
        }
        self.current
            .push_str(&format!("BT\n/F1 {size:.1} Tf\n{x:.2} {y:.2} Td\n<{h}> Tj\nET\n"));
        if bold {
            self.current.push_str("Q\n");
        }
    }

    pub(super) fn add_space(&mut self, pts: f32) {
        self.cursor_y -= pts;
    }

    pub(super) fn draw_hline(&mut self, gray: f32, width: f32) {
        let x0 = self.opts.margin_left;
        let x1 = x0 + self.opts.content_width();
        self.current.push_str(&format!(
            "q {gray} G {width} w {x0:.2} {y:.2} m {x1:.2} {y:.2} l S Q\n",
            y = self.cursor_y
        ));
        self.cursor_y -= 1.0;
    }

    pub(super) fn finalize(mut self) -> (Vec<String>, usize, GlyphSet, Vec<ImageObj>, Vec<BlockCoord>) {
        self.finish_page();
        let total = self.pages.len();
        (self.pages, total, self.glyphs, self.images, self.coords)
    }
}
