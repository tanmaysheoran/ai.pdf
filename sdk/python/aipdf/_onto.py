from __future__ import annotations

import xml.etree.ElementTree as ET

from ._pdf import sanitize_xml
from ._markdown import text_of


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
    # `rows` is pre-serialized with raw ^ / | delimiters and must NOT be
    # scalar-encoded again (mirrors the Rust core's column_raw handling).
    _onto_columns(lines, "Tables", table_rows, ["id", "page", "bbox", "caption", "rows"], raw_fields={"rows"})
    lines.append("")
    _onto_columns(lines, "Figures", figures, ["id", "page", "bbox", "caption", "alt", "source"])
    lines.append("")
    _onto_columns(lines, "References", references, ["id", "type", "text"])
    return "\n".join(lines).rstrip()


def _onto_columns(
    lines: list[str],
    name: str,
    records: list[dict[str, object]],
    fields: list[str],
    raw_fields: set[str] = frozenset(),
) -> None:
    lines.append(f"{name}[{len(records)}]:")
    for field in fields:
        if field in raw_fields:
            cells = (str(record.get(field, "")) for record in records)
        else:
            cells = (_onto_scalar(str(record.get(field, ""))) for record in records)
        lines.append(f"    {field}: " + "|".join(cells))


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
