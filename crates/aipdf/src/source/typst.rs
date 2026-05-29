use super::html::HtmlConverter;
use super::wrap_document;

// ── Typst → semantic XML ──────────────────────────────────────────────────────
//
// A pragmatic line-based converter. It covers the common Typst block
// constructs; full Typst (scripting, `#let`, templates, content functions,
// math layout) is out of scope and unsupported markup is flattened to text.

pub(crate) fn typst_to_xml(input: &str) -> String {
    let mut conv = HtmlConverter::new();
    let mut para: Vec<String> = Vec::new();
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let t = line.trim();

        if t.is_empty() {
            flush_typst_para(&mut conv, &mut para);
            i += 1;
            continue;
        }

        // Fenced code: ```lang ... ```
        if let Some(rest) = t.strip_prefix("```") {
            flush_typst_para(&mut conv, &mut para);
            let lang = rest.trim();
            let lang = (!lang.is_empty()).then(|| lang.to_string());
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code.push(lines[i]);
                i += 1;
            }
            i += 1; // skip closing fence
            conv.push_code_block(&code.join("\n"), lang.as_deref());
            continue;
        }

        // Headings: one or more '=' then a space.
        if let Some((level, title)) = typst_heading(t) {
            flush_typst_para(&mut conv, &mut para);
            conv.open_section(level, &strip_typst_inline(title));
            i += 1;
            continue;
        }

        // Figure / image: `image("path")`, optionally inside `#figure(...)`.
        if let Some(src) = typst_image_src(t) {
            flush_typst_para(&mut conv, &mut para);
            let caption = typst_caption(t);
            conv.push_figure(&src, "", caption.as_deref());
            i += 1;
            continue;
        }

        // Block equation: a line beginning with `$`.
        if t.starts_with('$') {
            flush_typst_para(&mut conv, &mut para);
            let mut buf = vec![t.to_string()];
            if !(t.len() > 1 && t.ends_with('$')) {
                i += 1;
                while i < lines.len() {
                    let lt = lines[i].trim();
                    buf.push(lt.to_string());
                    if lt.ends_with('$') {
                        break;
                    }
                    i += 1;
                }
            }
            i += 1;
            let joined = buf.join(" ");
            let inner = joined.trim().trim_matches('$').trim();
            conv.push_equation(inner);
            continue;
        }

        // Lists: consecutive `- ` (unordered) or `+ ` (ordered) items.
        if is_typst_list_item(t) {
            flush_typst_para(&mut conv, &mut para);
            let ordered = t.starts_with("+ ");
            let mut items = Vec::new();
            while i < lines.len() {
                let lt = lines[i].trim();
                if is_typst_list_item(lt) {
                    items.push(strip_typst_inline(lt[2..].trim()));
                    i += 1;
                } else {
                    break;
                }
            }
            conv.push_list(&items, ordered);
            continue;
        }

        para.push(t.to_string());
        i += 1;
    }
    flush_typst_para(&mut conv, &mut para);

    let blocks = conv.finish();
    if blocks.is_empty() {
        return wrap_document(vec![
            r#"<section id="s1" level="1" page="1">"#.to_string(),
            r#"<paragraph id="b1" page="1" role="paragraph">Document</paragraph>"#.to_string(),
            "</section>".to_string(),
        ]);
    }
    wrap_document(blocks)
}

pub(crate) fn flush_typst_para(conv: &mut HtmlConverter, para: &mut Vec<String>) {
    if !para.is_empty() {
        let text = para.join(" ");
        conv.push_paragraph(&strip_typst_inline(&text));
        para.clear();
    }
}

pub(crate) fn typst_heading(t: &str) -> Option<(usize, &str)> {
    let eqs = t.chars().take_while(|c| *c == '=').count();
    if eqs >= 1 && t.as_bytes().get(eqs) == Some(&b' ') {
        Some((eqs.min(6), t[eqs + 1..].trim()))
    } else {
        None
    }
}

pub(crate) fn is_typst_list_item(t: &str) -> bool {
    t.starts_with("- ") || t.starts_with("+ ")
}

/// Extract the first `image("...")` source path from a line, if present.
pub(crate) fn typst_image_src(t: &str) -> Option<String> {
    let start = t.find("image(")? + "image(".len();
    let rest = &t[start..];
    let q1 = rest.find('"')? + 1;
    let q2 = rest[q1..].find('"')? + q1;
    Some(rest[q1..q2].to_string())
}

/// Extract a `caption: [..]` value from a `#figure(...)` line, if present.
pub(crate) fn typst_caption(t: &str) -> Option<String> {
    let start = t.find("caption:")? + "caption:".len();
    let rest = t[start..].trim_start();
    let open = rest.find('[')? + 1;
    let close = rest[open..].find(']')? + open;
    Some(strip_typst_inline(rest[open..close].trim()))
}

/// Flatten light Typst inline markup. Emphasis markers `*` and inline-code
/// backticks are removed; other content (including `_`, which is common inside
/// identifiers) is preserved.
pub(crate) fn strip_typst_inline(s: &str) -> String {
    s.chars().filter(|c| *c != '*' && *c != '`').collect()
}
