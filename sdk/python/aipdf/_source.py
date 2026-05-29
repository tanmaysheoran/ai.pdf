from __future__ import annotations

# ---------------------------------------------------------------------------
# Source conversion helpers
# ---------------------------------------------------------------------------

import re
import xml.etree.ElementTree as ET

from ._models import AIPDFError
from ._pdf import sanitize_xml, validate_xml


def build_from_source(source: str, kind: str) -> str:
    """
    Convert source text to semantic XML.

    kind must be one of: 'xml', 'markdown', 'html', 'typst'.
    Returns the semantic XML string (does NOT produce a .aipdf file — that's
    the Rust CLI's job).
    """
    if kind not in ("xml", "markdown", "html", "typst"):
        raise AIPDFError(f"unsupported source kind: {kind!r}; expected one of xml, markdown, html, typst")

    if kind == "xml":
        # Strip optional ```xml … ``` fences
        stripped = source.strip()
        if stripped.startswith("```"):
            lines = stripped.splitlines()
            # Drop opening fence line and closing fence line
            inner = lines[1:-1] if lines[-1].strip() == "```" else lines[1:]
            stripped = "\n".join(inner)
        sanitize_xml(stripped)
        validate_xml(stripped)
        return stripped
    elif kind == "markdown":
        return markdown_to_xml(source)
    elif kind == "html":
        from ._source_html import html_to_xml
        return html_to_xml(source)
    elif kind == "typst":
        return typst_to_xml(source)


def _make_xml_document(sections: list[dict]) -> str:
    """Serialise a list of section dicts into a <document> XML string.

    Each section dict has keys: id, title (str), level (int), items (list of
    (tag, attrib_dict, text) tuples).
    """
    doc = ET.Element("document", version="1.0")
    for sec in sections:
        sec_elem = ET.SubElement(doc, "section", id=sec["id"], level=str(sec.get("level", 1)))
        if sec.get("title"):
            title_elem = ET.SubElement(sec_elem, "title")
            title_elem.text = sec["title"]
        for tag, attribs, text in sec.get("items", []):
            child = ET.SubElement(sec_elem, tag, **attribs)
            if text:
                child.text = text
    return ET.tostring(doc, encoding="unicode", xml_declaration=False)


def markdown_to_xml(source: str) -> str:
    """Convert Markdown source to aipdf semantic XML."""
    sections: list[dict] = []
    current_section: dict | None = None
    sec_counter = 0

    def _flush_or_create(title: str, level: int) -> None:
        nonlocal sec_counter, current_section
        sec_counter += 1
        current_section = {
            "id": f"s{sec_counter}",
            "title": title,
            "level": level,
            "items": [],
        }
        sections.append(current_section)

    def _ensure_section() -> dict:
        nonlocal current_section
        if current_section is None:
            _flush_or_create("", 1)
        return current_section  # type: ignore[return-value]

    lines = source.splitlines()
    i = 0
    while i < len(lines):
        line = lines[i]
        # ATX headings
        m = re.match(r"^(#{1,6})\s+(.*)", line)
        if m:
            level = len(m.group(1))
            title = m.group(2).strip()
            _flush_or_create(title, level)
            i += 1
            continue
        # Setext headings
        if i + 1 < len(lines) and re.match(r"^[=]{2,}\s*$", lines[i + 1]):
            _flush_or_create(line.strip(), 1)
            i += 2
            continue
        if i + 1 < len(lines) and re.match(r"^[-]{2,}\s*$", lines[i + 1]):
            _flush_or_create(line.strip(), 2)
            i += 2
            continue
        # Fenced code block
        m_fence = re.match(r"^```(\w*)", line)
        if m_fence:
            lang = m_fence.group(1)
            code_lines: list[str] = []
            i += 1
            while i < len(lines) and not lines[i].startswith("```"):
                code_lines.append(lines[i])
                i += 1
            i += 1  # skip closing fence
            sec = _ensure_section()
            attribs = {"language": lang} if lang else {}
            sec["items"].append(("codeBlock", attribs, "\n".join(code_lines)))
            continue
        # Blockquote
        if line.startswith("> "):
            text = line[2:].strip()
            _ensure_section()["items"].append(("citation", {}, text))
            i += 1
            continue
        # Unordered list
        if re.match(r"^[-*+]\s+", line):
            list_items: list[str] = []
            while i < len(lines) and re.match(r"^[-*+]\s+", lines[i]):
                list_items.append(re.sub(r"^[-*+]\s+", "", lines[i]).strip())
                i += 1
            sec = _ensure_section()
            # Encode as individual items grouped under a list placeholder
            for item_text in list_items:
                sec["items"].append(("item", {}, item_text))
            continue
        # Ordered list
        if re.match(r"^\d+\.\s+", line):
            list_items = []
            while i < len(lines) and re.match(r"^\d+\.\s+", lines[i]):
                list_items.append(re.sub(r"^\d+\.\s+", "", lines[i]).strip())
                i += 1
            sec = _ensure_section()
            for item_text in list_items:
                sec["items"].append(("item", {}, item_text))
            continue
        # Non-empty paragraph
        stripped = line.strip()
        if stripped:
            _ensure_section()["items"].append(("paragraph", {}, stripped))
        i += 1

    if not sections:
        sections.append({"id": "s1", "title": "", "level": 1, "items": []})

    xml_str = _make_xml_document(sections)
    validate_xml(xml_str)
    return xml_str


def typst_to_xml(source: str) -> str:
    """Convert Typst source to aipdf semantic XML (best-effort structural extraction)."""
    sections: list[dict] = []
    current_section: dict | None = None
    sec_counter = 0

    def _ensure_section() -> dict:
        nonlocal current_section
        if current_section is None:
            _flush("", 1)
        return current_section  # type: ignore[return-value]

    def _flush(title: str, level: int) -> None:
        nonlocal sec_counter, current_section
        sec_counter += 1
        current_section = {"id": f"s{sec_counter}", "title": title, "level": level, "items": []}
        sections.append(current_section)

    lines = source.splitlines()
    i = 0
    while i < len(lines):
        line = lines[i]
        # Typst headings: = Title, == Sub, === Sub-sub …
        m = re.match(r"^(={1,6})\s+(.*)", line)
        if m:
            level = len(m.group(1))
            _flush(m.group(2).strip(), level)
            i += 1
            continue
        # Code blocks: ```lang … ```
        m_fence = re.match(r"^```(\w*)", line)
        if m_fence:
            lang = m_fence.group(1)
            code_lines: list[str] = []
            i += 1
            while i < len(lines) and not lines[i].startswith("```"):
                code_lines.append(lines[i])
                i += 1
            i += 1
            attribs = {"language": lang} if lang else {}
            _ensure_section()["items"].append(("codeBlock", attribs, "\n".join(code_lines)))
            continue
        stripped = line.strip()
        # Skip Typst directives (#set, #let, #import …)
        if stripped.startswith("#"):
            i += 1
            continue
        if stripped:
            _ensure_section()["items"].append(("paragraph", {}, stripped))
        i += 1

    if not sections:
        sections.append({"id": "s1", "title": "", "level": 1, "items": []})

    xml_str = _make_xml_document(sections)
    validate_xml(xml_str)
    return xml_str
