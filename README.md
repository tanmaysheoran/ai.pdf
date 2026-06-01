<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
  <img src="assets/logo-light.svg" alt="ai.pdf" width="300">
</picture>

[![Release](https://github.com/tanmaysheoran/ai.pdf/actions/workflows/release.yml/badge.svg)](https://github.com/tanmaysheoran/ai.pdf/actions/workflows/release.yml)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey.svg)](#installation)

**An AI-native PDF extension format.**

*One file. Two audiences. Zero compromise.*

</div>

---

An `.ai.pdf` file is a fully valid PDF — it renders, prints, and opens in any PDF reader unchanged. It also embeds a Brotli-compressed semantic XML layer so AI-aware parsers can read structure directly, without OCR or heuristics.

> [!IMPORTANT]
> **Core rule:** PDF is the visual authority. XML is the machine-structure authority. Nothing is duplicated.

---

## Contents

- [Why](#why)
- [Installation](#installation)
- [How It Works](#how-it-works)
- [Format at a Glance](#format-at-a-glance)
- [Inputs](#inputs)
- [Export Formats](#export-formats)
- [CLI](#cli)
- [SDKs](#sdks)
- [Security](#security)
- [Status](#status)

---

## Why

PDF is the dominant format for structured documents — papers, contracts, reports, forms. But PDF was designed for print fidelity, not machine parsing. Today, AI systems either:

- Run OCR on rendered pixels (slow, lossy, layout-dependent), or
- Use brittle text extraction that loses tables, reading order, and structure.

`.ai.pdf` solves this by making the semantic layer a first-class citizen of the file itself.

---

## Installation

### Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/tanmaysheoran/ai.pdf/main/install.sh | sh
```

Checks for Rust/Cargo (installs via [rustup](https://rustup.rs) if absent), then downloads the pre-built binary or builds from source into `~/.cargo/bin`.

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/tanmaysheoran/ai.pdf/main/install.ps1 | iex
```

Downloads the pre-built `aipdf-x86_64-windows.zip`, extracts `aipdf.exe` to `%USERPROFILE%\.cargo\bin`, and adds it to your user `PATH`. Restart your terminal after install.

### From source

```bash
git clone https://github.com/tanmaysheoran/ai.pdf.git
cd ai.pdf
./install.sh          # detects local source automatically
```

```bash
# or directly with Cargo
cargo install --path crates/aipdf-cli
```

### Verify

```bash
aipdf --help
```

> [!TIP]
> If `aipdf` is not found after install, add `~/.cargo/bin` to your `PATH`:
> ```bash
> export PATH="$HOME/.cargo/bin:$PATH"
> ```

---

## How It Works

Every `.ai.pdf` file is a standard PDF with an invisible second layer:

| Layer | Object | Purpose |
|:------|:-------|:--------|
| **Visual** | PDF page tree + content streams (embedded Unicode font) | Human rendering |
| **Semantic** | `aipdf-semantic.xml.br` (embedded associated file) | Machine parsing |
| **Metadata** | XMP packet (`/Metadata`) | Discovery, indexing |

The embedded file is a Brotli-compressed XML document validated against the V1 schema. The PDF `/AF` (Associated Files) entry links the catalog to it. Legacy readers ignore the associated file entirely.

**Detection** works in two passes: a fast literal byte-scan for `/Subtype /application#2Faipdf+xml+br` (files written by this tool), falling back to a structural `lopdf` lookup via the Filespec `/EF` reference — which is what makes third-party re-saved PDFs detectable.

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

**Block types:** `paragraph`, `title`, `codeBlock`, `equation`, `table`, `figure`, `list`, `definitionList`, `note`, `footnote`, `annotation`, `citation`, `reference`

Each block can carry `id`, `page`, `bbox` (page-local coordinates), and `role` attributes.

---

## Inputs

The CLI and all SDKs accept these source formats and convert them to semantic XML automatically:

| Input | Notes |
|:------|:------|
| `.xml` | Direct semantic XML — must conform to the V1 schema |
| `.html` | HTML5 — headings, tables, lists, code, figures extracted |
| `.md` | Markdown via `pulldown-cmark` — headings, paragraphs, ordered/unordered lists, GFM tables, fenced code with language, blockquotes, images |
| `.typ` | Typst — headings, lists, fenced code, `$…$` equations, `image()` figures |
| existing `.pdf` | `aipdf ingest` — text extraction with optional `tesseract` OCR fallback for scanned pages |

---

## Export Formats

Once a file has a semantic layer, export to any of these:

| Format | Flag | Use case |
|:-------|:-----|:---------|
| XML | `--format xml` | Raw semantic payload |
| Markdown | `--format markdown` | Human-readable rendering |
| Markdown AST | `--format markdown-ast` | MDAST-compatible JSON tree |
| **ONTO** | `--format onto` | Token-efficient LLM ingestion |

> [!NOTE]
> **ONTO** is a columnar format (cells `^`-separated, rows `|`-separated) encoding the document as `Document`, `Sections`, `Blocks`, `Tables`, `Figures`, and `References` column families. Designed for direct LLM context injection with minimal token overhead, produced byte-identically by all three SDKs (verified by shared golden fixtures).

---

## CLI

```bash
# Build from source files
aipdf build samples/minimal.xml
aipdf build page.html --render full
aipdf build paper.md -o paper.ai.pdf
aipdf build paper.md --render full --font /path/to/NotoSansCJK.ttf   # embed a CJK font
aipdf build page.html --render browser -o page.ai.pdf                 # full CSS via headless Chrome

# Ingest an existing PDF (attach a semantic layer)
aipdf ingest scanned.pdf -o scanned.ai.pdf   # OCR scanned pages if tesseract is installed
aipdf ingest report.pdf --ocr never

# Inspect and validate
aipdf inspect paper.ai.pdf
aipdf validate paper.ai.pdf

# Extract and export
aipdf extract paper.ai.pdf
aipdf export paper.ai.pdf --format markdown
aipdf export paper.ai.pdf --format onto
aipdf export paper.ai.pdf --format markdown-ast
```

**Render modes:** `--render minimal` (plain text, fast) · `--render full` (layout engine with headings, tables, images) · `--render browser` (HTML only, full CSS via headless Chrome)

**Page sizes:** `--page-size letter` (default) · `--page-size a4`

### MCP server

The Python package ships an MCP stdio server for agent integration:

```bash
aipdf-mcp   # or: python -m aipdf.mcp_server
```

Tools exposed: `aipdf_inspect`, `aipdf_extract`, `aipdf_reading_order`, `aipdf_validate`, `aipdf_build`, `aipdf_convert`. See [`docs/mcp.md`](docs/mcp.md) for client configuration.

---

## SDKs

All three SDKs produce byte-identical output for read-side transforms (ONTO, Markdown, Markdown AST), enforced by shared golden fixtures in `tests/conformance/`. Write-side operations (`build`, `ingest`) delegate to the installed `aipdf` binary so the Rust core remains authoritative.

### Rust

```toml
[dependencies]
aipdf = { path = "crates/aipdf" }
```

```rust
use aipdf::AipdfDocument;

let doc = AipdfDocument::open("paper.ai.pdf")?;

println!("{}", doc.to_markdown()?);
println!("{}", doc.to_onto()?);

for block in doc.get_reading_order()? {
    println!("[{}] page={:?} {}", block.kind, block.page, block.text);
}
```

```rust
use aipdf::{build_aipdf, BuildOptions, RenderMode};

let xml = std::fs::read_to_string("paper.xml")?;
let bytes = build_aipdf(&xml, &BuildOptions {
    title: "My Paper".into(),
    render: RenderMode::Full,
    ..Default::default()
})?;
std::fs::write("paper.ai.pdf", bytes)?;
```

### Python

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

Dependency: `brotli >= 1.1.0`. No other runtime deps. Also ships `aipdf-mcp` — see [`docs/mcp.md`](docs/mcp.md).

### TypeScript / Node

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

No runtime dependencies. Uses Node's built-in `zlib` for Brotli and a small built-in XML parser.

---

## Security

The semantic layer is **data, not behavior**.

- Every XML path runs through `sanitize_xml` on both build and extract, with an identical rejected-marker list across Rust, Python, and TypeScript.
- **Rejected markers** (active-content / structural only): `<!DOCTYPE`, `<?xml-stylesheet`, `<?processing`, `<script`, `/JavaScript`, `/Launch`. Body text is XML-escaped, so these only match real markup.
- Natural-language phrases are **not** banned — XML text is data, and the visible PDF already carries the same words.
- No external entity resolution. Decompressed payload capped at **16 MiB**.
- No embeddings, prompts, model output, or executable content in the V1 format.

See [`docs/security.md`](docs/security.md) for the full threat model.

---

## Status

**V1 prototype.** The format spec, schema, and all three SDKs are functional and tested. Feedback on the schema, ONTO format, and PDF object layout is welcome via [issues](https://github.com/tanmaysheoran/ai.pdf/issues).

**Working today:**
- Authoring from XML, HTML, Markdown, and Typst
- `full` render with a real layout engine, embedded Unicode (CID/Type0) font, and raster figure images
- Per-block page + bbox coordinates written back into the embedded XML
- Ingestion of existing PDFs (text extraction + optional `tesseract` OCR)
- `1.x` version negotiation (forward-compatible: unknown elements ignored, other majors rejected)
- MCP server for agent integration
- Byte-identical ONTO/Markdown/AST across Rust, Python, and TypeScript (enforced by golden fixtures)

**Not yet:**
- Rasterizing vector PDF pages for OCR (only embedded-image scans are covered)
- Glyph subsetting for embedded fonts
- Signed payloads, incremental-update authoring, PDF 2.0 conformance suite

---

<details>
<summary><strong>Repository layout</strong></summary>

```
crates/
  aipdf/              Rust core library
    src/pdf.rs          PDF assembly, detection, extract/inspect
    src/render.rs       Full-render layout engine (page/bbox, images)
    src/browser.rs      Browser render path (headless Chrome)
    src/font.rs         Embedded CID/Type0 Unicode font
    src/ingest.rs       Ingest existing PDFs (text + OCR fallback)
    src/{xml,markdown,onto,source,security}.rs
    assets/             Bundled DejaVuSans.ttf (+ FONT-LICENSE.md)
  aipdf-cli/          CLI (clap, thin wrapper over the library)
sdk/
  python/             Pure-Python SDK (+ aipdf.mcp_server)
  typescript/         ESM TypeScript SDK (Node), proper XML parser
schema/
  aipdf-1.0.xsd       Normative XSD schema for the semantic payload
docs/
  spec.md             Full format specification (incl. versioning contract)
  security.md         Threat model and security controls
  compatibility.md    Legacy PDF reader compatibility notes
  mcp.md              MCP server tools and client configuration
samples/
  minimal.xml         Minimal valid semantic XML
  maximal.xml         All element types exercised (v1.0)
  html/ xml/          Comprehensive HTML / XML samples
tests/
  conformance/        Golden ONTO/Markdown fixtures (single source of truth)
  *.py                Python integration, smoke, MCP, and conformance tests
benches/              Compression analysis
```

</details>

---

## License

Apache 2.0 — see [LICENSE](LICENSE).
