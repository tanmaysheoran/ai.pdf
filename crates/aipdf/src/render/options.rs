use crate::font::Font;

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

// ── Text measurement (font-driven) ────────────────────────────────────────────

pub(crate) fn wrap_words(font: &Font, text: &str, size: f32, max_width: f32) -> Vec<String> {
    if text.trim().is_empty() {
        return vec![];
    }
    let space_w = font.text_width(" ", size);
    let mut lines = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0.0f32;

    for word in text.split_whitespace() {
        let w = font.text_width(word, size);
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
