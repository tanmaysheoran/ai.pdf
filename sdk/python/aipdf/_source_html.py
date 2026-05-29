from __future__ import annotations

import html.parser

from ._source import _make_xml_document
from ._pdf import validate_xml


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
