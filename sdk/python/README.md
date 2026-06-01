# aipdf Python SDK

Python SDK for reading and writing `.ai.pdf` files — valid PDFs with an embedded Brotli-compressed semantic XML layer.

The **read path** is pure Python (no binary needed). The **write path** (`build`, `ingest`, image extraction) delegates to the `aipdf` Rust CLI.

## Installation

```bash
pip install aipdf           # or: pip install -e sdk/python (editable from source)
```

Requires Python ≥ 3.10 and [`brotli`](https://pypi.org/project/Brotli/) ≥ 1.1.0 (installed automatically).

## Quick start

```python
from aipdf import AIPDF

doc = AIPDF.open("report.ai.pdf")

if doc.has_semantic_layer:
    print(doc.to_markdown())
```

## Read API (no binary required)

### Opening a file

```python
from aipdf import AIPDF, AIPDFDocument

doc: AIPDFDocument = AIPDF.open("report.ai.pdf")
```

### `AIPDFDocument` properties and methods

| Member | Returns | Description |
|--------|---------|-------------|
| `doc.has_semantic_layer` | `bool` | Whether the file has an embedded semantic layer |
| `doc.path` | `Path \| None` | Backing file path |
| `doc.xml` | `str \| None` | Raw semantic XML (if present) |
| `doc.inspect()` | `InspectReport` | Byte-level report (compressed size, XML size) |
| `doc.validate()` | `bool` | Validate the embedded XML; raises `AIPDFError` on failure |
| `doc.to_xml()` | `str` | Raw semantic XML string |
| `doc.to_markdown()` | `str` | Markdown rendering of the document |
| `doc.to_markdown_ast()` | `str` | MDAST-compatible JSON tree |
| `doc.to_onto()` | `str` | Columnar ONTO format for token-efficient LLM ingestion |
| `doc.get_structure()` | `list[SemanticBlock]` | Blocks in reading order |
| `doc.get_reading_order()` | `list[SemanticBlock]` | Alias for `get_structure()` |
| `doc.get_tables()` | `list[str]` | Text content of all `<table>` elements |
| `doc.find_citations()` | `list[str]` | Text content of all `<citation>` elements |

### `SemanticBlock`

```python
@dataclass
class SemanticBlock:
    kind: str        # element tag: "heading", "paragraph", "table", …
    id: str | None   # stable section/block id
    page: int | None # page number (set by `--render full`)
    bbox: str | None # bounding box string (set by `--render full`)
    text: str        # plain-text content
```

### `InspectReport`

```python
@dataclass
class InspectReport:
    is_pdf: bool
    has_semantic_layer: bool
    semantic_compressed_bytes: int | None
    semantic_xml_bytes: int | None
```

## Write API (requires `aipdf` CLI)

These functions shell out to the `aipdf` binary.

**Install the CLI** (one-liner):

```bash
curl -fsSL https://raw.githubusercontent.com/tanmaysheoran/ai.pdf/main/install.sh | sh
```

Or from a cloned repo:

```bash
./install.sh          # or: cargo install --path crates/aipdf-cli
```

Point to a custom binary via the `AIPDF_BIN` environment variable (default: `aipdf`).

### `build` — create a `.ai.pdf` from source

```python
from aipdf import build

# From Markdown, HTML, XML, or Typst
path = build("paper.md", "paper.ai.pdf", render="full", page_size="letter")
path = build("page.html", render="browser")          # full CSS fidelity via headless Chrome
path = build("doc.md", font="/path/to/NotoSansCJK.ttf")  # custom font face
```

| Parameter | Values | Default |
|-----------|--------|---------|
| `render` | `"minimal"`, `"full"`, `"browser"` | `"minimal"` |
| `page_size` | `"letter"`, `"a4"` | `"letter"` |
| `font` | path to a TrueType `.ttf` file | bundled DejaVu Sans |
| `title` | document title string | — |
| `output` | output path | auto-derived |

### `ingest` — attach a semantic layer to an existing PDF

```python
from aipdf import ingest

path = ingest("scanned.pdf", "scanned.ai.pdf")
path = ingest("report.pdf", ocr="never")
path = ingest("foreign.pdf", lang="eng+deu")
```

| Parameter | Values | Default |
|-----------|--------|---------|
| `ocr` | `"auto"`, `"never"`, `"force"` | `"auto"` |
| `lang` | tesseract language code(s) | `"eng"` |

### `export` — export the semantic layer

```python
from aipdf import export

text = export("doc.ai.pdf", fmt="markdown")          # returns str
result = export("doc.ai.pdf", fmt="onto", save="/out") # writes files, returns ExportResult
```

`ExportResult.output` — path to the content file  
`ExportResult.images` — paths to any extracted raster images

### `extract_images` — pull embedded raster images

```python
from aipdf import extract_images

image_paths = extract_images("doc.ai.pdf", "/out/images")
```

### `bench` — timing report

```python
from aipdf import bench

timings: dict[str, str] = bench("doc.ai.pdf")
```

## Low-level helpers

The following module-level functions are also exported for direct use:

```python
from aipdf import (
    xml_to_markdown,          # str → str
    xml_to_markdown_ast_json, # str → str (JSON)
    xml_to_onto,              # str → str
    get_reading_order,        # str → list[SemanticBlock]
    inspect_pdf,              # bytes → InspectReport
    markdown_to_xml,          # str → str
    html_to_xml,              # str → str
    typst_to_xml,             # str → str
    build_from_source,        # str, ext → str (XML)
)
```

## MCP server

The package ships an [MCP](https://modelcontextprotocol.io/) stdio server that exposes the same tools to AI agents:

```bash
aipdf-mcp                        # starts the MCP server on stdio
python -m aipdf.mcp_server       # alternative
```

Tools available: `aipdf_inspect`, `aipdf_extract`, `aipdf_reading_order`, `aipdf_validate` (native); `aipdf_build`, `aipdf_convert`, `aipdf_extract_images`, `aipdf_bench` (CLI-delegating).

See `docs/mcp.md` for client configuration.

## Error handling

All errors raise `AIPDFError`:

```python
from aipdf import AIPDF, AIPDFError

try:
    doc = AIPDF.open("file.pdf")
    print(doc.to_markdown())
except AIPDFError as e:
    print(f"error: {e}")
```
