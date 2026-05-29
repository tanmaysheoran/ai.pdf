from __future__ import annotations

import xml.etree.ElementTree as ET

from ._pdf import sanitize_xml


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
                lines.append(f"_{cap}_")
                lines.append("")
        elif child.tag == "definitionList":
            for defn in child.findall("definition"):
                term = defn.attrib.get("term", "")
                lines.append(f"- {term}: {text_of(defn)}")
            lines.append("")
        else:
            render_children(child, lines, level)


def render_table(table: ET.Element, lines: list[str]) -> None:
    cap_elem = table.find("caption")
    if cap_elem is not None:
        cap = text_of(cap_elem)
        if cap:
            lines.append(f"_{cap}_")
            lines.append("")
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
