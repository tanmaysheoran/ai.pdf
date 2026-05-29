from __future__ import annotations

# --- Markdown AST (MDAST) export ----------------------------------------------
# Mirrors the Rust core's streaming walker (`xml_to_markdown_ast` in markdown.rs)
# so the JSON output is byte-for-byte identical. Node dicts are built with keys
# in the Rust struct's field order (type, value, depth, lang, ordered, url, alt,
# children); `json.dumps(..., ensure_ascii=False)` then matches serde_json's
# pretty output (raw UTF-8, no key omission surprises).

import json
import xml.etree.ElementTree as ET

from ._pdf import sanitize_xml


def xml_to_markdown_ast_json(xml: str) -> str:
    """Serialize the document's MDAST tree as pretty JSON (Rust-conformant)."""
    return json.dumps(xml_to_markdown_ast(xml), indent=2, ensure_ascii=False)


def xml_to_markdown_ast(xml: str) -> dict:
    root = ET.fromstring(sanitize_xml(xml))
    children: list[dict] = []
    _ast_emit(root, children, {"level": 1})
    return {"type": "root", "children": children}


def _ast_text(elem: ET.Element) -> str:
    # Rust concatenates each trimmed text run (no internal whitespace
    # normalization), unlike `text_of`. For single-run elements they coincide.
    return "".join(seg.strip() for seg in elem.itertext())


def _ast_emit(elem: ET.Element, out: list[dict], state: dict) -> None:
    for child in elem:
        tag = child.tag
        if tag == "section":
            try:
                state["level"] = int(child.attrib.get("level"))
            except (TypeError, ValueError):
                state["level"] = 1
            _ast_emit(child, out, state)
        elif tag == "title":
            out.append(_ast_heading(state["level"], _ast_text(child)))
        elif tag == "paragraph":
            out.append(_ast_paragraph(_ast_text(child)))
        elif tag == "caption":
            out.append(_ast_paragraph(_ast_text(child)))
        elif tag == "citation":
            out.append(_ast_blockquote(_ast_text(child)))
        elif tag == "equation":
            out.append(_ast_value("math", _ast_text(child)))
        elif tag == "codeBlock":
            out.append(_ast_value("code", _ast_text(child), child.attrib.get("language")))
        elif tag == "note":
            out.append(_ast_blockquote(f"Note: {_ast_text(child)}"))
        elif tag == "footnote":
            out.append({"type": "footnoteDefinition", "children": [_ast_paragraph(_ast_text(child))]})
        elif tag == "image":
            out.append(_ast_image_paragraph(child.attrib.get("src", ""), child.attrib.get("alt", "")))
        elif tag in ("list", "references", "definitionList"):
            items: list[dict] = []
            for sub in child:
                if sub.tag in ("item", "reference"):
                    items.append(_ast_list_item(_ast_text(sub)))
                elif sub.tag == "definition":
                    term = sub.attrib.get("term", "")
                    value = _ast_text(sub) if not term else f"{term}: {_ast_text(sub)}"
                    items.append(_ast_list_item(value))
            out.append({"type": "list", "ordered": False, "children": items})
        elif tag == "table":
            cap = child.find("caption")
            if cap is not None:
                out.append(_ast_paragraph(_ast_text(cap)))
            rows = [[_ast_text(c) for c in row.findall("cell")] for row in child.iter("row")]
            out.append(_ast_table(rows))
        else:
            _ast_emit(child, out, state)


def _ast_text_node(value: str) -> dict:
    return {"type": "text", "value": value}


def _ast_heading(depth: int, value: str) -> dict:
    return {"type": "heading", "depth": min(max(depth, 1), 6), "children": [_ast_text_node(value)]}


def _ast_paragraph(value: str) -> dict:
    return {"type": "paragraph", "children": [_ast_text_node(value)]}


def _ast_image_paragraph(src: str, alt: str) -> dict:
    return {"type": "paragraph", "children": [{"type": "image", "url": src, "alt": alt}]}


def _ast_blockquote(value: str) -> dict:
    return {"type": "blockquote", "children": [_ast_paragraph(value)]}


def _ast_list_item(value: str) -> dict:
    return {"type": "listItem", "children": [_ast_paragraph(value)]}


def _ast_table(rows: list[list[str]]) -> dict:
    return {
        "type": "table",
        "children": [
            {"type": "tableRow", "children": [
                {"type": "tableCell", "children": [_ast_text_node(cell)]} for cell in row
            ]}
            for row in rows
        ],
    }


def _ast_value(node_type: str, value: str, lang: str | None = None) -> dict:
    node = {"type": node_type, "value": value}
    if lang is not None:
        node["lang"] = lang
    return node
