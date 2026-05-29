from __future__ import annotations

from pathlib import Path

# ---------------------------------------------------------------------------
# Re-export everything from the private submodules so that
#   from aipdf.core import X
# keeps working for every previously-importable name.
# ---------------------------------------------------------------------------

from ._models import (
    SEMANTIC_SUBTYPE,
    SEMANTIC_FILENAME,
    DISALLOWED_MARKERS,
    AIPDFError,
    SemanticBlock,
    InspectReport,
)

from ._pdf import (
    extract_semantic_xml,
    inspect_pdf,
    find_semantic_stream,
    validate_xml,
    sanitize_xml,
    get_reading_order,
    collect_element_text,
)

from ._markdown import (
    xml_to_markdown,
    render_children,
    render_table,
    text_of,
)

from ._ast import (
    xml_to_markdown_ast_json,
    xml_to_markdown_ast,
    _ast_text,
    _ast_emit,
    _ast_text_node,
    _ast_heading,
    _ast_paragraph,
    _ast_image_paragraph,
    _ast_blockquote,
    _ast_list_item,
    _ast_table,
    _ast_value,
)

from ._onto import (
    xml_to_onto,
    _onto_columns,
    _onto_field,
    _onto_scalar,
    _onto_array_scalar,
    default_onto_role,
    is_metadata_child,
)

from ._source import (
    build_from_source,
    _make_xml_document,
    markdown_to_xml,
    typst_to_xml,
)

from ._source_html import (
    _HTMLToXMLParser,
    html_to_xml,
)


class AIPDF:
    @staticmethod
    def open(path: str | Path) -> "AIPDFDocument":
        return AIPDFDocument.open(path)


from dataclasses import dataclass


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

    def inspect(self) -> InspectReport:
        """Byte-level report matching `aipdf inspect` (re-reads the file)."""
        if self.path is None:
            raise AIPDFError("document has no backing file path")
        return inspect_pdf(Path(self.path).read_bytes())

    def validate(self) -> bool:
        """Validate the embedded semantic XML (matches `aipdf validate`)."""
        validate_xml(self.to_xml())
        return True

    def to_xml(self) -> str:
        if self.xml is None:
            raise AIPDFError("semantic layer not found")
        return self.xml

    def get_structure(self) -> list[SemanticBlock]:
        return get_reading_order(self.to_xml())

    def to_markdown(self) -> str:
        return xml_to_markdown(self.to_xml())

    def to_markdown_ast(self) -> str:
        return xml_to_markdown_ast_json(self.to_xml())

    def to_onto(self) -> str:
        return xml_to_onto(self.to_xml())

    def get_tables(self) -> list[str]:
        return collect_element_text(self.to_xml(), "table")

    def get_reading_order(self) -> list[SemanticBlock]:
        return get_reading_order(self.to_xml())

    def find_citations(self) -> list[str]:
        return collect_element_text(self.to_xml(), "citation")
