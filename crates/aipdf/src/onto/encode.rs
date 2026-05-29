pub(super) fn encode_scalar(value: &str) -> String {
    let normalized = normalize(value);
    let escaped = normalized
        .replace('`', "'")
        .replace('|', "/")
        .replace('^', ";");
    if escaped.contains('\n') || escaped.starts_with(' ') || escaped.ends_with(' ') {
        format!("`{}`", escaped.trim())
    } else {
        escaped
    }
}

pub(super) fn encode_array_scalar(value: &str) -> String {
    encode_scalar(value)
}

pub(super) fn normalize(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn default_role(name: &str) -> String {
    match name {
        "title" => "heading",
        "paragraph" => "body",
        "caption" => "caption",
        "equation" => "equation",
        "citation" => "citation",
        "item" => "list-item",
        "note" => "note",
        "footnote" => "footnote",
        "definition" => "definition",
        "codeBlock" => "code",
        "annotation" => "annotation",
        _ => name,
    }
    .to_string()
}
