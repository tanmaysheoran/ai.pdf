# ai.pdf

**An AI-native PDF extension format.**

An `.ai.pdf` file is a fully valid PDF — it renders, prints, and opens in any PDF reader unchanged. It also embeds a Brotli-compressed semantic XML layer as an associated file so AI-aware parsers can read structure directly, without OCR or heuristics.

**Core rule:** PDF is the visual authority. XML is the machine-structure authority. Nothing is duplicated.

---

## Why

PDF is the dominant format for structured documents — papers, contracts, reports, forms. But PDF was designed for print fidelity, not machine parsing. Today, AI systems either:

- Run OCR on rendered pixels (slow, lossy, layout-dependent), or  
- Use brittle text extraction that loses tables, reading order, and structure.

`.ai.pdf` solves this by making the semantic layer a first-class citizen of the file itself. One file. Two audiences.

---

## How It Works

Every `.ai.pdf` file contains:

| Layer | Object | Purpose |
|---|---|---|
| Visual | PDF page tree + content streams (with an embedded Unicode font) | Human rendering |
| Semantic | `aipdf-semantic.xml.br` (embedded file) | Machine parsing |
| Metadata | XMP packet (`/Metadata`) | Discovery, indexing |

The embedded file is a Brotli-compressed XML document validated against the V1 schema. The PDF `/AF` (Associated Files) entry links the catalog to the semantic file. Legacy readers ignore the associated file entirely.

Detection: primarily by the embedded filename `aipdf-semantic.xml.br` reached through the Filespec `/EF` reference; a fast literal byte-scan for `/Subtype /application#2Faipdf+xml+br` (the conformant escape of MIME `application/aipdf+xml+br`) is tried first for files written by this tool.

---

## Format at a Glance

```xml
<document version="1.0">
  <metadata>
    <title>Research Summary</title>
    <authors><author>Jane Smith</author></authors>
  </metadata>

  <section id="s-intro" level="1">
    <title>Introduction</title>
    <paragraph id="p1" page="1" bbox="72,680,540,710">
      This paper proposes a new approach to...
    </paragraph>
    <figure id="fig1">
      <image src="architecture.png" alt="System architecture diagram"/>
      <caption>Figure 1. System overview.</caption>
    </figure>
  </section>

  <section id="s-results" level="1">
    <title>Results</title>
    <table id="t1">
      <caption>Table 1. Benchmark results.</caption>
      <thead><row><cell header="true">Method</cell><cell header="true">Score</cell></row></thead>
      <tbody><row><cell>Baseline</cell><cell>71.2</cell></row></tbody>
    </table>
  </section>
</document>
```

Block types: `paragraph`, `title`, `codeBlock`, `equation`, `table`, `figure`, `list`, `definitionList`, `note`, `footnote`, `annotation`, `citation`, `reference`.

Each block can carry `id`, `page`, `bbox` (page-local coordinates), and `role` attributes.

---

## Inputs

The CLI and SDKs accept multiple source formats and convert them to semantic XML automatically:

| Input | Notes |
|---|---|
| `.xml` | Direct semantic XML — must conform to the V1 schema |
| `.html` | HTML5 — headings, tables, lists, code, figures extracted |
| `.md` | Markdown (via `pulldown-cmark`) — headings, paragraphs, ordered/unordered lists, GFM tables, fenced code with language, blockquotes, images |
| `.typ` | Typst — headings, lists, fenced code, `$…$` equations, `image()` figures |
| existing `.pdf` | `aipdf ingest` extracts text (with an optional `tesseract` OCR fallback for scanned pages) and attaches a semantic layer to the original |

---

## Export Formats

Once a file has a semantic layer, you can export to:

| Format | Flag | Use |
|---|---|---|
| XML | `--format xml` | Raw semantic payload |
| Markdown | `--format markdown` | Human-readable rendering |
| Markdown AST | `--format markdown-ast` | MDAST-compatible JSON tree |
| ONTO | `--format onto` | Columnar token-efficient LLM ingestion |

**ONTO** is a columnar format (cells `^`-separated, rows `|`-separated) that encodes the document as `Document`, `Sections`, `Blocks`, `Tables`, `Figures`, and `References` column families. It is designed for direct LLM context injection with minimal token overhead, and is produced identically by all three SDKs (verified against shared golden fixtures).

---

## CLI

```bash
# Build from XML, HTML, or Markdown
aipdf build samples/minimal.xml
aipdf build samples/comprehensive.html --render full
aipdf build paper.md -o paper.ai.pdf
aipdf build paper.md --render full --font /path/to/NotoSansCJK.ttf  # embed a CJK font

# Ingest an existing PDF (extract text + optional OCR, attach semantic layer)
aipdf ingest scanned.pdf -o scanned.ai.pdf            # OCR scanned pages if tesseract is installed
aipdf ingest report.pdf --ocr never

# Inspect and validate
aipdf inspect paper.ai.pdf
aipdf validate paper.ai.pdf

# Extract the raw semantic XML
aipdf extract paper.ai.pdf

# Export to various formats
aipdf export paper.ai.pdf --format markdown
aipdf export paper.ai.pdf --format onto
aipdf export paper.ai.pdf --format markdown-ast
```

Render modes: `--render minimal` (plain text page, fast) or `--render full` (laid-out PDF with headings, tables, code blocks).

Page sizes: `--page-size letter` (default) or `--page-size a4`.

### MCP server (agent integration)

The Python package ships an MCP stdio server so agents can read `.ai.pdf`
structure directly (`aipdf_inspect`, `aipdf_extract`, `aipdf_reading_order`):

```bash
aipdf-mcp          # or: python -m aipdf.mcp_server
```

See [`docs/mcp.md`](docs/mcp.md) for client configuration.

---

## Rust

```toml
[dependencies]
aipdf = { path = "crates/aipdf" }
```

```rust
use aipdf::AipdfDocument;

// Open an existing .ai.pdf file
let doc = AipdfDocument::open("paper.ai.pdf")?;

println!("{}", doc.to_markdown()?);
println!("{}", doc.to_onto()?);

for block in doc.get_reading_order()? {
    println!("[{}] page={:?} {}", block.kind, block.page, block.text);
}

// Build from XML source
use aipdf::{build_aipdf, BuildOptions, RenderMode};

let xml = std::fs::read_to_string("paper.xml")?;
let bytes = build_aipdf(&xml, &BuildOptions {
    title: "My Paper".into(),
    render: RenderMode::Full,
    ..Default::default()
})?;
std::fs::write("paper.ai.pdf", bytes)?;
```

---

## Python

```bash
pip install -e sdk/python
```

```python
from aipdf import AIPDF

doc = AIPDF.open("paper.ai.pdf")

print(doc.to_xml())
print(doc.to_markdown())
print(doc.to_onto())

for block in doc.get_reading_order():
    print(f"[{block.kind}] page={block.page}  {block.text[:60]}")
```

Dependency: `brotli >= 1.1.0`. No other runtime deps. The package also ships an MCP server (`aipdf-mcp` / `python -m aipdf.mcp_server`) — see [`docs/mcp.md`](docs/mcp.md).

---

## TypeScript / Node

```bash
cd sdk/typescript && npm install && npm run build
```

```ts
import { AIPDF } from "./src/index.js";

const doc = AIPDF.open("paper.ai.pdf");

console.log(doc.toMarkdown());
console.log(doc.toOnto());

for (const block of doc.getReadingOrder()) {
  console.log(`[${block.kind}] page=${block.page}  ${block.text.slice(0, 60)}`);
}
```

No runtime dependencies. Uses Node's built-in `zlib` for Brotli and a small built-in XML parser for the semantic layer.

---

## Security

The semantic layer is **data, not behavior**.

- Every XML path runs through `sanitize_xml` before use (both on build and on extract), with the same rejected-marker list across the Rust core and both SDKs.
- Rejected strings: `<!DOCTYPE`, `<?xml-stylesheet`, `<?processing`, `<script`, `/JavaScript`, `/Launch`, `prompt:`, `system prompt`, `model directive`.
- No external entity resolution.
- Decompressed payload capped at 16 MiB.
- The embedded filename is fixed to `aipdf-semantic.xml.br`.
- No embeddings, prompts, model output, or executable content are part of the V1 format.

---

## Repository Layout

```
crates/
  aipdf/          Rust core library
    src/pdf.rs      PDF assembly, detection, extract/inspect
    src/render.rs   Full-render layout engine (page/bbox, images)
    src/font.rs     Embedded CID/Type0 Unicode font
    src/ingest.rs   Ingest existing PDFs (text + OCR fallback)
    src/{xml,markdown,onto,source,security}.rs
    assets/         Bundled DejaVuSans.ttf (+ FONT-LICENSE.md)
  aipdf-cli/      CLI (clap, thin wrapper over the library)
sdk/
  python/         Pure-Python SDK (+ aipdf.mcp_server)
  typescript/     ESM TypeScript SDK (Node), proper XML parser
schema/
  aipdf-1.0.xsd   Normative XSD schema for the semantic payload
docs/
  spec.md         Full format specification (incl. versioning contract)
  security.md     Threat model and security controls
  compatibility.md  Legacy PDF reader compatibility notes
  mcp.md          MCP server tools and client configuration
samples/
  minimal.xml     Minimal valid semantic XML
  maximal.xml     All element types exercised (v1.0)
  html/ xml/      Comprehensive HTML / XML samples
tests/
  conformance/    Golden ONTO/Markdown fixtures (single source of truth)
  *.py / Rust + JS integration + round-trip + fuzz + MCP tests
benches/          Compression analysis
```

---

## Getting Started

```bash
git clone https://github.com/tanmaysheoran/ai.pdf.git
cd ai.pdf

# Rust
cargo test
cargo run -p aipdf-cli -- build samples/minimal.xml
cargo run -p aipdf-cli -- inspect samples/minimal.ai.pdf

# Python
python3 -m venv .venv
.venv/bin/pip install -e sdk/python
.venv/bin/python tests/python_smoke.py

# TypeScript
cd sdk/typescript && npm install && npm test
```

---

## Status

V1 prototype. The format specification, schema, and all three SDKs are functional and tested. This is an early design — feedback on the schema, the ONTO format, and the PDF object layout is welcome via issues.

Working today: authoring from XML/HTML/Markdown/Typst; `full` render with a real layout engine, an embedded Unicode (CID/Type0) font, and embedded figure images; real per-block page/bbox coordinates; ingestion of existing PDFs (text extraction + optional `tesseract` OCR); `1.x` version negotiation; an MCP server for agents; and byte-identical ONTO/Markdown across the Rust, Python, and TypeScript implementations (enforced by shared golden fixtures).

Not yet in this repository: rasterizing vector PDF pages for OCR (only embedded-image scans are covered), glyph subsetting for embedded fonts, signed payloads, incremental-update authoring, or a PDF 2.0 conformance test suite. OCR requires the `tesseract` CLI to be installed.

---

## License

Apache 2.0 — see [LICENSE](LICENSE).
