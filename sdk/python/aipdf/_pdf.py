from __future__ import annotations

import re
import xml.etree.ElementTree as ET

try:
    import brotli
except ImportError as exc:  # pragma: no cover
    brotli = None
    _brotli_import_error = exc

from ._models import (
    AIPDFError,
    DISALLOWED_MARKERS,
    InspectReport,
    SEMANTIC_SUBTYPE,
    SemanticBlock,
)


def extract_semantic_xml(data: bytes) -> str | None:
    stream = find_semantic_stream(data)
    if stream is None:
        return None
    if brotli is None:  # pragma: no cover
        raise AIPDFError(f"brotli dependency is required: {_brotli_import_error}")
    xml = brotli.decompress(stream).decode("utf-8")
    validate_xml(xml)
    return xml


def inspect_pdf(data: bytes) -> InspectReport:
    """Report PDF / semantic-layer presence and byte sizes (matches Rust `inspect_pdf`)."""
    is_pdf = data.startswith(b"%PDF-")
    stream = find_semantic_stream(data)
    if stream is None:
        return InspectReport(is_pdf, False, None, None)
    if brotli is None:  # pragma: no cover
        raise AIPDFError(f"brotli dependency is required: {_brotli_import_error}")
    try:
        # Mirror Rust's decompress_semantic: sanitize (trim) then measure UTF-8 bytes.
        xml = sanitize_xml(brotli.decompress(stream).decode("utf-8"))
        return InspectReport(is_pdf, True, len(stream), len(xml.encode("utf-8")))
    except AIPDFError:
        return InspectReport(is_pdf, False, len(stream), None)


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
    xml = xml.lstrip("﻿").strip()
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
