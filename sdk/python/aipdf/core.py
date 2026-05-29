from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Iterable
import html.parser
import re
import xml.etree.ElementTree as ET

try:
    import brotli
except ImportError as exc:  # pragma: no cover
    brotli = None
    _brotli_import_error = exc


SEMANTIC_SUBTYPE = b"/application#aipdf+xml+br"
SEMANTIC_FILENAME = "aipdf-semantic.xml.br"
DISALLOWED_MARKERS = (
    "<!DOCTYPE",
    "<?xml-stylesheet",
    "<script",
    "/JavaScript",
    "/Launch",
    "system prompt",
    "model directive",
)


class AIPDFError(Exception):
    pass


@dataclass(frozen=True)
class SemanticBlock:
    kind: str
    id: str | None
    page: int | None
    bbox: str | None
    text: str


class AIPDF:
    @staticmethod
    def open(path: str | Path) -> "AIPDFDocument":
        return AIPDFDocument.open(path)


@dataclass
class AIPDFDocument:
    path: Path | None
    xml: str | None
    is_pdf: bool = True

    @classmethod
    def open(cls, path: str | Path) -> "AIPDFDocument":
        path = Path(path)
        data = path.read_bytes()
        xml = extract_semantic_xml(data)
        return cls(path=path, xml=xml, is_pdf=data.startswith(b"%PDF-"))

    @property
    def has_semantic_layer(self) -> bool:
        return self.xml is not None

    def to_xml(self) -> str:
        if self.xml is None:
            raise AIPDFError("semantic layer not found")
        return self.xml

    def get_structure(self) -> list[SemanticBlock]:
        return get_reading_order(self.to_xml())

    def to_markdown(self) -> str:
        return xml_to_markdown(self.to_xml())

    def to_onto(self) -> str:
        return xml_to_onto(self.to_xml())

    def get_tables(self) -> list[str]:
        return collect_element_text(self.to_xml(), "table")

    def get_reading_order(self) -> list[SemanticBlock]:
        return get_reading_order(self.to_xml())

    def find_citations(self) -> list[str]:
        return collect_element_text(self.to_xml(), "citation")


def extract_semantic_xml(data: bytes) -> str | None:
    stream = find_semantic_stream(data)
    if stream is None:
        return None
    if brotli is None:  # pragma: no cover
        raise AIPDFError(f"brotli dependency is required: {_brotli_import_error}")
    xml = brotli.decompress(stream).decode("utf-8")
    validate_xml(xml)
    return xml


def find_semantic_stream(data: bytes) -> bytes | None:
    marker_pos = data.find(SEMANTIC_SUBTYPE)
    if marker_pos < 0:
        return None
    stream_pos = data.find(b"stream\n", marker_pos)
    if stream_pos < 0:
        return None
    start = stream_pos + len(b"stream\n")
    end = data.find(b"\nendstream", start)
    if end < 0:
        return None
    return data[start:end]


def validate_xml(xml: str) -> None:
    sanitized = sanitize_xml(xml)
    try:
        root = ET.fromstring(sanitized)
    except ET.ParseError as exc:
        raise AIPDFError(f"invalid semantic XML: {exc}") from exc
    if root.tag != "document":
        raise AIPDFError("root element must be <document>")
    if not root.attrib.get("version"):
        raise AIPDFError("document version must be present")
    sections = root.findall(".//section")
    if not sections:
        raise AIPDFError("document must contain at least one section")
    for section in sections:
        if not section.attrib.get("id"):
            raise AIPDFError("section elements require stable id attributes")
    for elem in root.iter():
        bbox = elem.attrib.get("bbox")
        if bbox and not re.fullmatch(r"-?\d+(\.\d+)?,-?\d+(\.\d+)?,-?\d+(\.\d+)?,-?\d+(\.\d+)?", bbox):
            raise AIPDFError(f"invalid bbox: {bbox}")


def sanitize_xml(xml: str) -> str:
    xml = xml.lstrip("\ufeff").strip()
    lowered = xml.lower()
    for marker in DISALLOWED_MARKERS:
        if marker.lower() in lowered:
            raise AIPDFError(f"disallowed marker `{marker}`")
    if len(xml.encode("utf-8")) > 16 * 1024 * 1024:
        raise AIPDFError("semantic XML exceeds 16 MiB safety limit")
    return xml


def get_reading_order(xml: str) -> list[SemanticBlock]:
    root = ET.fromstring(sanitize_xml(xml))
    blocks: list[SemanticBlock] = []
    for elem in root.iter():
        if elem.tag in {"title", "paragraph", "caption", "equation", "citation", "cell",
                        "item", "codeBlock", "reference", "footnote", "note"}:
            text = "".join(elem.itertext()).strip()
            page = elem.attrib.get("page")
            blocks.append(
                SemanticBlock(
                    kind=elem.tag,
                    id=elem.attrib.get("id"),
                    page=int(page) if page else None,
                    bbox=elem.attrib.get("bbox"),
                    text=text,
                )
            )
    return blocks


def collect_element_text(xml: str, element: str) -> list[str]:
    root = ET.fromstring(sanitize_xml(xml))
    return [" ".join("".join(e.itertext()).split()) for e in root.iter(element)]


def xml_to_markdown(xml: str) -> str:
    root = ET.fromstring(sanitize_xml(xml))
    lines: list[str] = []
    render_children(root, lines, level=1)
    return "\n".join(lines).strip()


def render_children(elem: ET.Element, lines: list[str], level: int) -> None:
    for child in elem:
        if child.tag == "section":
            child_level = int(child.attrib.get("level", level))
            render_children(child, lines, child_level)
        elif child.tag == "title":
            lines.extend([f"{'#' * min(max(level, 1), 6)} {text_of(child)}", ""])
        elif child.tag == "paragraph":
            lines.extend([text_of(child), ""])
        elif child.tag == "citation":
            lines.extend([f"> {text_of(child)}", ""])
        elif child.tag == "equation":
            lines.extend(["```math", text_of(child), "```", ""])
        elif child.tag == "table":
            render_table(child, lines)
        elif child.tag == "list":
            ordered = child.attrib.get("type", "unordered") == "ordered"
            for i, item in enumerate(child.findall("item"), start=1):
                prefix = f"{i}." if ordered else "-"
                lines.append(f"{prefix} {text_of(item)}")
            lines.append("")
        elif child.tag == "codeBlock":
            lang = child.attrib.get("language", "")
            lines.extend([f"```{lang}", text_of(child), "```", ""])
        elif child.tag == "note":
            lines.extend([f"> Note: {text_of(child)}", ""])
        elif child.tag == "footnote":
            lines.extend([f"[^note]: {text_of(child)}", ""])
        elif child.tag == "references":
            for ref in child:
                if ref.tag == "reference":
                    lines.append(f"- {text_of(ref)}")
            lines.append("")
        elif child.tag == "figure":
            image = child.find("image")
            alt = child.attrib.get("alt", "")
            src = ""
            if image is not None:
                alt = alt or image.attrib.get("alt", "")
                src = image.attrib.get("src", "")
            cap_elem = child.find("caption")
            cap = text_of(cap_elem) if cap_elem is not None else ""
            lines.append(f"![{alt}]({src})")
            lines.append("")
            if cap:
                lines.append(cap)
                lines.append("")
        elif child.tag == "definitionList":
            for defn in child.findall("definition"):
                term = defn.attrib.get("term", "")
                lines.append(f"- {term}: {text_of(defn)}")
            lines.append("")
        else:
            render_children(child, lines, level)


def render_table(table: ET.Element, lines: list[str]) -> None:
    collected_rows: list[list[str]] = []
    thead = table.find("thead")
    if thead is not None:
        for row in thead.findall("row"):
            collected_rows.append([text_of(cell) for cell in row.findall("cell")])
    tbody = table.find("tbody")
    if tbody is not None:
        for row in tbody.findall("row"):
            collected_rows.append([text_of(cell) for cell in row.findall("cell")])
    # Direct <row> children not inside thead/tbody
    subgroup_tags = {"thead", "tbody"}
    for row in table.findall("row"):
        collected_rows.append([text_of(cell) for cell in row.findall("cell")])
    if not collected_rows:
        return
    lines.append("| " + " | ".join(collected_rows[0]) + " |")
    lines.append("| " + " | ".join("---" for _ in collected_rows[0]) + " |")
    for row in collected_rows[1:]:
        lines.append("| " + " | ".join(row) + " |")
    lines.append("")


def text_of(elem: ET.Element) -> str:
    return " ".join("".join(elem.itertext()).split())


def xml_to_onto(xml: str) -> str:
    root = ET.fromstring(sanitize_xml(xml))
    title_elem = root.find("./metadata/title")
    doc_title = text_of(title_elem) if title_elem is not None else ""
    sections: list[dict[str, str]] = []
    blocks: list[dict[str, str]] = []
    tables: list[dict[str, object]] = []
    figures: list[dict[str, str]] = []
    references: list[dict[str, str]] = []

    def walk(elem: ET.Element, section: dict[str, str] | None = None) -> None:
        tag = elem.tag
        current = section
        if tag in {"section", "appendix"}:
            current = {
                "id": elem.attrib.get("id", ""),
                "level": elem.attrib.get("level", "appendix" if tag == "appendix" else ""),
                "page": elem.attrib.get("page", elem.attrib.get("pageStart", "")),
                "role": elem.attrib.get("role", elem.attrib.get("semanticRole", "appendix" if tag == "appendix" else "")),
                "title": "",
            }
            sections.append(current)
        elif tag == "table":
            tables.append(
                {
                    "id": elem.attrib.get("id", ""),
                    "page": elem.attrib.get("page", ""),
                    "bbox": elem.attrib.get("bbox", ""),
                    "caption": text_of(elem.find("caption")) if elem.find("caption") is not None else "",
                    "rows": [[text_of(cell) for cell in row.findall("cell")] for row in elem.iter("row")],
                }
            )
            return
        elif tag == "figure":
            image = elem.find("image")
            figures.append(
                {
                    "id": elem.attrib.get("id", ""),
                    "page": elem.attrib.get("page", ""),
                    "bbox": elem.attrib.get("bbox", ""),
                    "caption": text_of(elem.find("caption")) if elem.find("caption") is not None else "",
                    "alt": image.attrib.get("alt", "") if image is not None else "",
                    "source": elem.attrib.get("source", image.attrib.get("src", "") if image is not None else ""),
                }
            )
            return
        elif tag == "reference":
            references.append({"id": elem.attrib.get("id", ""), "type": elem.attrib.get("type", ""), "text": text_of(elem)})
            return
        elif tag in {"title", "paragraph", "caption", "equation", "citation", "item", "note", "footnote", "definition", "codeBlock", "annotation"}:
            if current is not None and not is_metadata_child(elem, root):
                value = text_of(elem)
                if tag == "definition" and elem.attrib.get("term"):
                    value = f"{elem.attrib['term']}: {value}"
                role = elem.attrib.get("role", default_onto_role(tag))
                block = {
                    "id": elem.attrib.get("id", ""),
                    "kind": tag,
                    "section_id": current.get("id", ""),
                    "level": current.get("level", ""),
                    "page": elem.attrib.get("page", current.get("page", "")),
                    "bbox": elem.attrib.get("bbox", ""),
                    "role": role,
                    "text": value,
                }
                if tag == "title" and not current.get("title"):
                    current["title"] = value
                blocks.append(block)
        for child in elem:
            walk(child, current)

    walk(root)

    lines = ["Document[1]:"]
    _onto_field(lines, "version", root.attrib.get("version", ""))
    _onto_field(lines, "title", doc_title)
    _onto_field(lines, "source_format", "aipdf.semantic.xml")
    lines.append("")
    _onto_columns(lines, "Sections", sections, ["id", "level", "page", "role", "title"])
    lines.append("")
    _onto_columns(lines, "Blocks", blocks, ["id", "kind", "section_id", "level", "page", "bbox", "role", "text"])
    lines.append("")
    table_rows = [dict(t, rows="|".join("^".join(_onto_array_scalar(cell) for cell in row) for row in t["rows"])) for t in tables]
    _onto_columns(lines, "Tables", table_rows, ["id", "page", "bbox", "caption", "rows"])
    lines.append("")
    _onto_columns(lines, "Figures", figures, ["id", "page", "bbox", "caption", "alt", "source"])
    lines.append("")
    _onto_columns(lines, "References", references, ["id", "type", "text"])
    return "\n".join(lines).rstrip()


def _onto_columns(lines: list[str], name: str, records: list[dict[str, object]], fields: list[str]) -> None:
    lines.append(f"{name}[{len(records)}]:")
    for field in fields:
        lines.append(f"    {field}: " + "|".join(_onto_scalar(str(record.get(field, ""))) for record in records))


def _onto_field(lines: list[str], name: str, value: str) -> None:
    lines.append(f"    {name}: {_onto_scalar(value)}")


def _onto_scalar(value: str) -> str:
    clean = " ".join(value.split()).replace("`", "'").replace("|", "/").replace("^", ";")
    return clean


def _onto_array_scalar(value: str) -> str:
    return _onto_scalar(value)


def default_onto_role(tag: str) -> str:
    return {
        "title": "heading",
        "paragraph": "body",
        "caption": "caption",
        "equation": "equation",
        "citation": "citation",
        "item": "list-item",
        "note": "note",
        "footnote": "footnote",
        "definition": "definition",
        "codeBlock": "code",
        "annotation": "annotation",
    }.get(tag, tag)


def is_metadata_child(elem: ET.Element, root: ET.Element) -> bool:
    metadata = root.find("metadata")
    return metadata is not None and elem in list(metadata.iter())


# ---------------------------------------------------------------------------
# Source conversion helpers
# ---------------------------------------------------------------------------

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


class _HTMLToXMLParser(html.parser.HTMLParser):
    """SAX-style HTML → semantic XML converter (no external deps)."""

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self._sections: list[dict] = []
        self._current_section: dict | None = None
        self._sec_counter = 0
        # Stack entries: (tag, element_dict | None)
        self._stack: list[tuple[str, object]] = []
        self._text_buf: list[str] = []
        # list tracking
        self._list_stack: list[str] = []  # "ul" or "ol" per nesting level
        self._in_item = False
        self._item_text: list[str] = []
        # figure tracking
        self._in_figure = False
        self._figure_alt = ""
        self._figure_src = ""
        self._figure_cap: list[str] = []
        self._in_figcaption = False
        # table tracking
        self._in_table = False
        self._table_rows: list[list[str]] = []
        self._current_row: list[str] | None = None
        self._cell_text: list[str] = []
        self._in_cell = False
        # code tracking
        self._in_code = False
        self._in_pre = False
        self._code_text: list[str] = []
        # blockquote
        self._in_blockquote = False
        self._blockquote_text: list[str] = []

    def _ensure_section(self) -> dict:
        if self._current_section is None:
            self._sec_counter += 1
            self._current_section = {"id": f"s{self._sec_counter}", "title": "", "level": 1, "items": []}
            self._sections.append(self._current_section)
        return self._current_section

    def _add_item(self, tag: str, attribs: dict, text: str) -> None:
        self._ensure_section()["items"].append((tag, attribs, text.strip()))

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        attrd = dict(attrs)
        self._stack.append((tag, attrd))
        self._text_buf = []
        if tag in ("h1", "h2", "h3", "h4", "h5", "h6"):
            self._text_buf = []
        elif tag in ("ul", "ol"):
            self._list_stack.append(tag)
        elif tag == "li":
            self._in_item = True
            self._item_text = []
        elif tag in ("table",):
            self._in_table = True
            self._table_rows = []
        elif tag == "tr":
            self._current_row = []
        elif tag in ("th", "td"):
            self._in_cell = True
            self._cell_text = []
        elif tag == "blockquote":
            self._in_blockquote = True
            self._blockquote_text = []
        elif tag == "pre":
            self._in_pre = True
            self._code_text = []
        elif tag == "code" and self._in_pre:
            self._in_code = True
        elif tag == "figure":
            self._in_figure = True
            self._figure_alt = attrd.get("alt", "")
            self._figure_src = attrd.get("src", "")
            self._figure_cap = []
        elif tag == "img":
            if self._in_figure:
                self._figure_src = attrd.get("src", self._figure_src)
                self._figure_alt = attrd.get("alt", self._figure_alt)
        elif tag == "figcaption":
            self._in_figcaption = True
            self._figure_cap = []

    def handle_endtag(self, tag: str) -> None:
        if tag in ("h1", "h2", "h3", "h4", "h5", "h6"):
            level = int(tag[1])
            title = "".join(self._text_buf).strip()
            self._sec_counter += 1
            self._current_section = {
                "id": f"s{self._sec_counter}",
                "title": title,
                "level": level,
                "items": [],
            }
            self._sections.append(self._current_section)
        elif tag == "p":
            if self._in_blockquote:
                self._blockquote_text.extend(self._text_buf)
            elif self._in_figure and self._in_figcaption:
                self._figure_cap.extend(self._text_buf)
            elif not self._in_item and not self._in_cell:
                text = "".join(self._text_buf).strip()
                if text:
                    self._add_item("paragraph", {}, text)
        elif tag == "li":
            self._in_item = False
            text = "".join(self._item_text).strip()
            if text:
                self._add_item("item", {}, text)
        elif tag in ("ul", "ol"):
            if self._list_stack:
                self._list_stack.pop()
        elif tag == "blockquote":
            self._in_blockquote = False
            text = "".join(self._blockquote_text).strip()
            if text:
                self._add_item("citation", {}, text)
        elif tag == "pre":
            self._in_pre = False
            self._in_code = False
            text = "".join(self._code_text).strip()
            if text:
                self._add_item("codeBlock", {}, text)
        elif tag in ("th", "td"):
            self._in_cell = False
            if self._current_row is not None:
                self._current_row.append("".join(self._cell_text).strip())
        elif tag == "tr":
            if self._current_row is not None:
                self._table_rows.append(self._current_row)
                self._current_row = None
        elif tag == "table":
            self._in_table = False
            # Store table as paragraph-like placeholder (ONTO walker handles tables natively)
            # Here we encode it as a special marker for simplicity; a full table element
            # would need ET sub-element manipulation outside of this string-building flow.
            # We store a serialised representation as a paragraph note.
            if self._table_rows:
                header = " | ".join(self._table_rows[0]) if self._table_rows else ""
                rows_text = "\n".join(" | ".join(r) for r in self._table_rows)
                self._add_item("paragraph", {}, f"[Table]\n{rows_text}")
        elif tag == "figcaption":
            self._in_figcaption = False
        elif tag == "figure":
            self._in_figure = False
            cap_text = "".join(self._figure_cap).strip()
            self._add_item("paragraph", {}, f"![{self._figure_alt}]({self._figure_src})")
            if cap_text:
                self._add_item("caption", {}, cap_text)
        # pop stack
        if self._stack and self._stack[-1][0] == tag:
            self._stack.pop()

    def handle_data(self, data: str) -> None:
        if self._in_code or self._in_pre:
            self._code_text.append(data)
        elif self._in_cell:
            self._cell_text.append(data)
        elif self._in_item:
            self._item_text.append(data)
        elif self._in_blockquote:
            self._blockquote_text.append(data)
        elif self._in_figcaption:
            self._figure_cap.append(data)
        else:
            self._text_buf.append(data)


def html_to_xml(source: str) -> str:
    """Convert HTML source to aipdf semantic XML."""
    parser = _HTMLToXMLParser()
    parser.feed(source)
    sections = parser._sections
    if not sections:
        sections = [{"id": "s1", "title": "", "level": 1, "items": []}]
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
