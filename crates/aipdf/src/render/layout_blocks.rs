use super::image::{EncodedImage, ImageObj, load_image};
use super::layout::{Layout, BODY_LEAD, BODY_SIZE, CODE_LEAD, CODE_SIZE, PARA_SPACE, SECTION_SPACE};

impl Layout {
    // ── Renderers ────────────────────────────────────────────────────────────

    pub(super) fn render_doc_title(&mut self, text: &str) {
        let size = 22.0;
        let leading = 28.0;
        let lines = self.wrap(text, size, self.opts.content_width());
        let needed = lines.len() as f32 * leading + SECTION_SPACE * 2.0;
        self.ensure_space(needed);
        for line in &lines {
            let w = self.measure(line, size);
            let x = self.opts.margin_left + ((self.opts.content_width() - w) / 2.0).max(0.0);
            let y = self.cursor_y;
            self.draw_single(line, x, y, size, true);
            self.cursor_y -= leading;
        }
        self.add_space(SECTION_SPACE);
        self.draw_hline(0.5, 0.3);
        self.add_space(SECTION_SPACE);
    }

    pub(super) fn render_heading(&mut self, level: usize, text: &str) {
        let (size, leading, space_before) = match level {
            1 => (18.0f32, 24.0f32, 20.0f32),
            2 => (14.0, 19.0, 14.0),
            3 => (12.0, 17.0, 10.0),
            _ => (11.0, 15.4, 8.0),
        };
        let lines = self.wrap(text, size, self.opts.content_width());
        let needed = lines.len() as f32 * leading + space_before + 6.0;
        self.ensure_space(needed);
        self.add_space(space_before);
        self.draw_text_lines(&lines, self.opts.margin_left, size, leading, true);
        self.add_space(6.0);
        if level <= 2 {
            self.draw_hline(0.5, 0.6);
            self.add_space(4.0);
        }
    }

    pub(super) fn render_paragraph(&mut self, text: &str) {
        let lines = self.wrap(text, BODY_SIZE, self.opts.content_width());
        if lines.is_empty() {
            return;
        }
        let needed = lines.len() as f32 * BODY_LEAD + PARA_SPACE;
        self.ensure_space(needed);
        self.draw_text_lines(&lines, self.opts.margin_left, BODY_SIZE, BODY_LEAD, false);
        self.add_space(PARA_SPACE);
    }

    pub(super) fn render_citation(&mut self, text: &str) {
        let indent = 20.0;
        let x = self.opts.margin_left + indent;
        let w = self.opts.content_width() - indent;
        let lines = self.wrap(text, BODY_SIZE, w);
        if lines.is_empty() {
            return;
        }
        let block_h = lines.len() as f32 * BODY_LEAD;
        let needed = block_h + PARA_SPACE * 2.0;
        self.ensure_space(needed);
        self.add_space(PARA_SPACE / 2.0);
        let bar_x = self.opts.margin_left + 4.0;
        let bar_top = self.cursor_y + BODY_SIZE * 0.2;
        let bar_bot = self.cursor_y - block_h;
        self.current.push_str(&format!(
            "q 0.4 G 2 w {bar_x:.2} {bar_top:.2} m {bar_x:.2} {bar_bot:.2} l S Q\n"
        ));
        self.draw_text_lines(&lines, x, BODY_SIZE, BODY_LEAD, false);
        self.add_space(PARA_SPACE);
    }

    pub(super) fn render_note(&mut self, text: &str) {
        let prefixed = format!("Note: {text}");
        let lines = self.wrap(&prefixed, BODY_SIZE, self.opts.content_width() - 10.0);
        if lines.is_empty() {
            return;
        }
        let block_h = lines.len() as f32 * BODY_LEAD + 8.0;
        self.ensure_space(block_h + PARA_SPACE);
        let bx = self.opts.margin_left;
        let by = self.cursor_y - block_h + 4.0;
        self.current.push_str(&format!(
            "q 0.93 g {bx:.2} {by:.2} {:.2} {block_h:.2} re f Q\n",
            self.opts.content_width()
        ));
        self.add_space(4.0);
        self.draw_text_lines(&lines, bx + 6.0, BODY_SIZE, BODY_LEAD, false);
        self.add_space(PARA_SPACE);
    }

    pub(super) fn render_code_block(&mut self, text: &str, _language: Option<&str>) {
        let raw_lines: Vec<&str> = text.lines().collect();
        let code_width = self.opts.content_width() - 16.0;
        // Approximate a monospace cell from the embedded font's digit width.
        let char_w = self.measure("0", CODE_SIZE).max(1.0);
        let mut wrapped: Vec<String> = Vec::new();
        for raw in &raw_lines {
            if raw.is_empty() {
                wrapped.push(String::new());
            } else if self.measure(raw, CODE_SIZE) <= code_width {
                wrapped.push(raw.to_string());
            } else {
                let chars_per_line = (code_width / char_w).floor().max(1.0) as usize;
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
        self.current.push_str(&format!(
            "q 0.92 g {bx:.2} {by:.2} {w:.2} {block_h:.2} re f 0.75 G 0.5 w {bx:.2} {by:.2} {w:.2} {block_h:.2} re S Q\n",
            w = self.opts.content_width()
        ));
        self.add_space(6.0);
        self.draw_text_lines(&wrapped, bx + 8.0, CODE_SIZE, CODE_LEAD, false);
        self.add_space(6.0 + PARA_SPACE);
    }

    pub(super) fn render_list(&mut self, ordered: bool, items: &[String]) {
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
                    "\u{2022}".to_string()
                };
                let wrapped = self.wrap(item, BODY_SIZE, text_w);
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
            let by = self.cursor_y;
            self.draw_single(bullet, bullet_x, by, BODY_SIZE, false);
            self.draw_text_lines(lines, text_x, BODY_SIZE, BODY_LEAD, false);
        }
        self.add_space(PARA_SPACE);
    }

    pub(super) fn render_table(&mut self, caption: Option<&str>, rows: &[Vec<String>]) {
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
            self.finish_page();
            self.begin_page();
        }

        if let Some(cap) = caption {
            let lines = self.wrap(cap, BODY_SIZE - 1.0, self.opts.content_width());
            self.draw_text_lines(&lines, self.opts.margin_left, BODY_SIZE - 1.0, BODY_LEAD, true);
            self.add_space(PARA_SPACE / 2.0);
        }

        let tbl_x = self.opts.margin_left;

        for (ri, row) in rows.iter().enumerate() {
            let is_header = ri == 0;
            let y_top = self.cursor_y;
            let y_bot = y_top - row_h;

            if is_header {
                self.current.push_str(&format!(
                    "q 0.85 g {tbl_x:.2} {y_bot:.2} {:.2} {row_h:.2} re f Q\n",
                    self.opts.content_width()
                ));
            }

            for (ci, cell) in row.iter().enumerate() {
                let cx = tbl_x + ci as f32 * col_w;
                let text_x = cx + 4.0;
                let available_w = col_w - 8.0;
                let lines = self.wrap(cell, BODY_SIZE, available_w);
                let text = lines.first().cloned().unwrap_or_default();
                let text_y = y_top - BODY_SIZE - 3.0;
                self.draw_single(&text, text_x, text_y, BODY_SIZE, is_header);
                if ci < ncols - 1 {
                    let vx = cx + col_w;
                    self.current.push_str(&format!(
                        "q 0.5 G 0.5 w {vx:.2} {y_top:.2} m {vx:.2} {y_bot:.2} l S Q\n"
                    ));
                }
            }

            self.current.push_str(&format!(
                "q 0.4 G 0.5 w {tbl_x:.2} {y_bot:.2} m {:.2} {y_bot:.2} l S Q\n",
                tbl_x + self.opts.content_width()
            ));

            self.cursor_y -= row_h;

            if ri < rows.len() - 1 && self.available_y() < row_h * 2.0 {
                self.finish_page();
                self.begin_page();
            }
        }

        let tbl_top = self.cursor_y + rows.len() as f32 * row_h;
        let tbl_bot = self.cursor_y;
        self.current.push_str(&format!(
            "q 0.4 G 0.5 w {tbl_x:.2} {tbl_bot:.2} {w:.2} {h:.2} re S Q\n",
            w = self.opts.content_width(),
            h = tbl_top - tbl_bot
        ));

        self.add_space(PARA_SPACE);
    }

    pub(super) fn render_figure(&mut self, alt: &str, src: &str, caption: Option<&str>) {
        let bw = self.opts.content_width();
        // Try to embed the real raster; fall back to a labelled placeholder.
        match load_image(self.base_dir.as_deref(), src) {
            Some(enc) => self.draw_image(enc),
            None => self.draw_figure_placeholder(alt, src),
        }
        if let Some(cap) = caption {
            self.add_space(4.0);
            let cap_lines = self.wrap(cap, BODY_SIZE - 1.0, bw);
            for line in &cap_lines {
                let lw = self.measure(line, BODY_SIZE - 1.0);
                let lx = self.opts.margin_left + (bw - lw) / 2.0;
                let y = self.cursor_y;
                self.draw_single(line, lx, y, BODY_SIZE - 1.0, false);
                self.cursor_y -= BODY_LEAD;
            }
        }
        self.add_space(PARA_SPACE);
    }

    pub(super) fn draw_image(&mut self, enc: EncodedImage) {
        let bw = self.opts.content_width();
        // Scale to content width, preserving aspect ratio, capped to ~80% of
        // the printable page height.
        let aspect = enc.height as f32 / enc.width as f32;
        let mut dw = bw;
        let mut dh = dw * aspect;
        let max_h = (self.opts.height - self.opts.margin_top - self.opts.margin_bottom) * 0.8;
        if dh > max_h {
            dh = max_h;
            dw = dh / aspect;
        }
        self.ensure_space(dh + PARA_SPACE * 2.0);
        let name = format!("Im{}", self.images.len() + 1);
        let x = self.opts.margin_left + (bw - dw) / 2.0;
        let y = self.cursor_y - dh;
        // Image space is the unit square; cm scales/translates it into place.
        self.current
            .push_str(&format!("q {dw:.2} 0 0 {dh:.2} {x:.2} {y:.2} cm /{name} Do Q\n"));
        self.images.push(ImageObj { name, enc });
        self.cursor_y -= dh;
    }

    pub(super) fn draw_figure_placeholder(&mut self, alt: &str, src: &str) {
        let box_h = 80.0;
        self.ensure_space(box_h + PARA_SPACE * 2.0);
        let bx = self.opts.margin_left;
        let by = self.cursor_y - box_h;
        let bw = self.opts.content_width();
        self.current.push_str(&format!(
            "q 0.9 g {bx:.2} {by:.2} {bw:.2} {box_h:.2} re f 0.6 G 1 w {bx:.2} {by:.2} {bw:.2} {box_h:.2} re S Q\n"
        ));
        let label = if !alt.is_empty() {
            alt.to_string()
        } else if !src.is_empty() {
            format!("[Figure: {src}]")
        } else {
            "[Figure]".to_string()
        };
        let label_w = self.measure(&label, BODY_SIZE);
        let lx = bx + (bw - label_w) / 2.0;
        let ly = by + box_h / 2.0 - BODY_SIZE / 2.0;
        self.draw_single(&label, lx, ly, BODY_SIZE, false);
        self.cursor_y -= box_h;
    }
}
