from __future__ import annotations

from dataclasses import dataclass


# Conformant PDF name for MIME application/aipdf+xml+br ('/' escaped as #2F).
SEMANTIC_SUBTYPE = b"/application#2Faipdf+xml+br"
SEMANTIC_FILENAME = "aipdf-semantic.xml.br"
# Active-content / structural markers only. Kept in lockstep with the Rust core
# (security.rs) and TypeScript SDK. Natural-language phrases are intentionally
# NOT banned: XML text is data, never instructions, and the visible PDF already
# carries the same words.
DISALLOWED_MARKERS = (
    "<!DOCTYPE",
    "<?xml-stylesheet",
    "<?processing",
    "<script",
    "/JavaScript",
    "/Launch",
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


@dataclass(frozen=True)
class InspectReport:
    """Mirror of the Rust core's `InspectReport` (pdf.rs) / `aipdf inspect`."""

    is_pdf: bool
    has_semantic_layer: bool
    semantic_compressed_bytes: int | None
    semantic_xml_bytes: int | None
