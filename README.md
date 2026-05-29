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
| Visual | PDF page tree + content streams | Human rendering |
| Semantic | `aipdf-semantic.xml.br` (embedded file) | Machine parsing |
| Metadata | XMP packet in PDF stream object 6 | Discovery, indexing |

The embedded file is a Brotli-compressed XML document validated against the V1 schema. The PDF `/AF` (Associated Files) entry links the catalog to the semantic file. Legacy readers ignore the associated file entirely.

Detection: scan for `/Subtype /application#aipdf+xml+br` in the PDF byte stream.

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
| `.md` | Markdown — headings, paragraphs, code fences, tables |
| `.typ` | Typst source |

---

## Export Formats

Once a file has a semantic layer, you can export to:

| Format | Flag | Use |
|---|---|---|
| XML | `--format xml` | Raw semantic payload |
| Markdown | `--format markdown` | Human-readable rendering |
| Markdown AST | `--format markdown-ast` | MDAST-compatible JSON tree |
| ONTO | `--format onto` | Columnar token-efficient LLM ingestion |

**ONTO** is a tab-separated columnar format that encodes the document as `Document`, `Sections`, `Blocks`, `Tables`, `Figures`, and `References` rows. It is designed for direct LLM context injection with minimal token overhead.

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

for block in doc.reading_order():
    print(f"[{block.kind}] page={block.page}  {block.text[:60]}")
```

Dependency: `brotli >= 1.1.0`. No other runtime deps.

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

for (const block of doc.readingOrder()) {
  console.log(`[${block.kind}] page=${block.page}  ${block.text.slice(0, 60)}`);
}
```

No runtime dependencies. Uses Node's built-in `zlib` for Brotli.

---

## Security

The semantic layer is **data, not behavior**.

- Every XML path runs through `sanitize_xml` before use (both on build and on extract).
- Rejected strings: `<!DOCTYPE`, `<?xml-stylesheet`, `<script`, `/JavaScript`, `/Launch`, `system prompt`, `model directive`.
- No external entity resolution.
- Decompressed payload capped at 16 MiB.
- The embedded filename is fixed to `aipdf-semantic.xml.br`.
- No embeddings, prompts, model output, or executable content are part of the V1 format.

---

## Repository Layout

```
crates/
  aipdf/          Rust core library (format logic, export, security)
  aipdf-cli/      CLI (clap, thin wrapper over the library)
sdk/
  python/         Pure-Python SDK
  typescript/     ESM TypeScript SDK (Node)
schema/
  aipdf-1.0.xsd   Normative XSD schema for the semantic payload
docs/
  spec.md         Full format specification
  security.md     Threat model and security controls
  compatibility.md  Legacy PDF reader compatibility notes
samples/
  minimal.xml     Minimal valid semantic XML
  maximal.xml     All element types exercised
  html/           Comprehensive HTML sample + CSS
  xml/            Comprehensive XML sample + CSS
tests/            Integration tests (Rust, Python, JS)
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

Not yet in this repository: a production-grade PDF renderer, signed payloads, incremental update support, or a PDF 2.0 conformance test suite.

---

## License

Apache 2.0 — see [LICENSE](LICENSE).
