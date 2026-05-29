# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Project Is

`ai.pdf` is a prototype AI-native PDF extension format. An `.ai.pdf` file is a fully valid PDF — it renders, prints, and opens in any PDF reader unchanged. It also embeds a Brotli-compressed semantic XML layer as an associated file (`aipdf-semantic.xml.br`) so AI-aware parsers can read structure directly, without OCR or heuristics.

The invariant: **PDF is the visual authority. The embedded XML is the machine-structure authority. Nothing is duplicated.**

Detection (two paths, in `pdf.rs`): a fast literal byte-scan for `/Subtype /application#2Faipdf+xml+br` (files written by this crate's hand builder), falling back to a structural `lopdf` lookup that finds the EmbeddedFile via the Filespec `/EF` reference by the filename `aipdf-semantic.xml.br`. The fallback is what makes ingested / third-party-re-saved PDFs detectable, since PDF tools re-encode the subtype name. (The subtype is the conformant escape of MIME `application/aipdf+xml+br`; an earlier non-conformant `#aipdf` form caused readers to drop the embedded file.)

## Commands

### Rust (core library + CLI)
```bash
cargo test                    # run all Rust tests
cargo test -p aipdf           # test core library only

# Build (accepts .xml, .html, .md, .typ)
cargo run -p aipdf-cli -- build samples/minimal.xml -o samples/minimal.ai.pdf
cargo run -p aipdf-cli -- build samples/minimal.xml --render full --page-size letter
cargo run -p aipdf-cli -- build paper.md --render full --font /path/to/NotoSansCJK.ttf  # embed a custom face
cargo run -p aipdf-cli -- build page.html --render browser -o page.ai.pdf  # full CSS fidelity (needs Chrome)

# Ingest an existing PDF (text extraction + optional OCR), attaching a semantic layer
cargo run -p aipdf-cli -- ingest scanned.pdf -o scanned.ai.pdf            # --ocr auto (default)
cargo run -p aipdf-cli -- ingest report.pdf --ocr never --lang eng

# Inspect, validate, extract
cargo run -p aipdf-cli -- inspect samples/minimal.ai.pdf
cargo run -p aipdf-cli -- validate samples/minimal.ai.pdf
cargo run -p aipdf-cli -- extract samples/minimal.ai.pdf

# Export formats
cargo run -p aipdf-cli -- export samples/minimal.ai.pdf --format xml
cargo run -p aipdf-cli -- export samples/minimal.ai.pdf --format markdown
cargo run -p aipdf-cli -- export samples/minimal.ai.pdf --format markdown-ast
cargo run -p aipdf-cli -- export samples/minimal.ai.pdf --format onto
```

### Python SDK
```bash
python3 -m venv .venv
.venv/bin/python -m pip install -e sdk/python
.venv/bin/python tests/python_smoke.py
```

### TypeScript SDK
```bash
cd sdk/typescript
npm install
npm run build
npm test
```

### MCP server (Python)
```bash
.venv/bin/python tests/mcp_smoke.py          # drive the stdio server over JSON-RPC
aipdf-mcp                                     # or: python -m aipdf.mcp_server
```

### Cross-SDK conformance
```bash
cargo test -p aipdf --test conformance        # Rust vs golden fixtures
.venv/bin/python tests/conformance_python.py  # Python vs same goldens
cd sdk/typescript && npm test                 # TS vs same goldens (conformance.test.mjs)
```
Goldens live in `tests/conformance/` and are authored by the Rust core; all three implementations must reproduce them byte-for-byte.

## Architecture

The workspace has two crates:

- **`crates/aipdf`** — core library. All format logic lives here.
- **`crates/aipdf-cli`** — thin CLI wrapper using `clap` that calls into `crates/aipdf`.

### Core library modules (`crates/aipdf/src/`)

| Module | Responsibility |
|--------|---------------|
| `lib.rs` | Public API: `AipdfDocument`, re-exports (`Font`, `ingest_pdf`, `SUPPORTED_MAJOR_VERSION`, …), `AipdfError` |
| `pdf.rs` | PDF byte assembly (`build_aipdf`), Brotli compress/decompress, `extract_semantic_xml`, `inspect_pdf`. Minimal render = hand-written PDF (14 objects incl. embedded font); detection = literal byte-scan + `lopdf` structural fallback. |
| `render.rs` | `--render full` layout engine: parses semantic XML, lays out headings/paragraphs/lists/tables/code/figures, records per-block page+bbox, embeds raster images, then assembles the PDF object tree. Writes the laid-out coordinates back into the embedded XML before compression. Self-contained; ignores CSS. |
| `browser.rs` | `--render browser` (HTML input only): renders the original HTML — stylesheet and all — to a PDF with **headless Chrome** (shelled out, like `ingest`'s tesseract), then attaches the semantic layer to that PDF via `ingest::attach_semantic_layer`. Gives full CSS fidelity (colours, backgrounds, borders, web fonts, table striping) at the cost of a Chrome/Chromium runtime dependency. `chrome_available()` reports presence; `AIPDF_CHROME` overrides the binary path. The visible layer is the browser's; the embedded XML stays semantic (no per-block bbox write-back on this path). |
| `font.rs` | Embedded CID/Type0 TrueType font support (Identity-H + ToUnicode + per-glyph widths). Vendors `assets/DejaVuSans.ttf` as the default face; `Font::from_path` loads a custom (e.g. CJK) face. Used by both renderers so non-ASCII survives in the visible layer. |
| `ingest.rs` | `ingest_pdf` — parse an existing PDF with `lopdf`, extract text per page (OCR fallback via the `tesseract` CLI for scanned pages), and attach the semantic layer to the **original** document. `IngestOptions { ocr: OcrMode, lang }`. |
| `xml.rs` | XML validation + version negotiation (`check_supported_version`, accepts `1.x`), `get_reading_order` → `Vec<SemanticBlock>`, `get_tables`, `find_citations`, `SUPPORTED_MAJOR_VERSION` |
| `markdown.rs` | `xml_to_markdown` (rendered string; honours ordered lists, code languages, figure images) and `xml_to_markdown_ast_json` (MDAST-compatible JSON) |
| `onto.rs` | `xml_to_onto` — columnar ONTO-style export for token-efficient LLM ingestion. Emits `Document`, `Sections`, `Blocks`, `Tables`, `Figures`, `References` in source order. Table rows use `^` (cell separator) and `|` (row separator); pre-serialized rows are joined by `column_raw` without re-encoding. |
| `security.rs` | `sanitize_xml` — rejects disallowed markers (DOCTYPE, JS, prompt/model-directive strings), enforces 16 MiB size cap |
| `source.rs` | `semantic_xml_from_source` — converts Markdown (via `pulldown-cmark`), HTML (`scraper`), Typst (line-based), or raw XML inputs into valid semantic XML |

External crates of note: `lopdf` (ingest + structural detection), `ttf-parser` + `flate2` (font embedding), `image` (figure rasters), `pulldown-cmark` (Markdown), `scraper` (HTML).

### Data flow

```
Input (XML/MD/HTML/Typst)
  → source::semantic_xml_from_source
  → security::sanitize_xml
  → xml::validate_xml   (incl. version negotiation: accept 1.x)
  → pdf::build_aipdf
      • minimal: Brotli compress → embed in hand-written PDF (font embedded)
      • full:    render::build_rendered_pdf → lay out + record page/bbox →
                 write coords back into XML → Brotli compress → embed (+ images)
      • browser: (HTML only) browser::build_aipdf_browser → headless Chrome
                 renders HTML+CSS → PDF → attach semantic layer via lopdf
  → .ai.pdf file

Existing PDF
  → ingest::ingest_pdf   (lopdf text extract, OCR fallback) → attach layer to original → .ai.pdf

.ai.pdf file
  → pdf::extract_semantic_xml   (literal byte-scan, else lopdf structural lookup → Brotli decompress → sanitize + validate)
  → markdown::xml_to_markdown / onto::xml_to_onto / xml::get_reading_order
  → (agents) sdk/python aipdf.mcp_server over MCP stdio
```

### CLI options

`build` options:

| Option | Values | Default |
|--------|--------|---------|
| `--render` | `minimal` (plain text, fast), `full` (laid-out PDF with headings, tables, code blocks, images), `browser` (HTML input only — full CSS fidelity via headless Chrome) | `minimal` |
| `--page-size` | `letter`, `a4` | `letter` |
| `--font` | path to a TrueType face to embed (e.g. a Noto CJK font) | bundled DejaVu Sans |

`ingest` options:

| Option | Values | Default |
|--------|--------|---------|
| `--ocr` | `auto` (OCR low-text pages), `never`, `force` | `auto` |
| `--lang` | tesseract language code(s), e.g. `eng`, `eng+deu` | `eng` |

`ingest` OCR shells out to the `tesseract` CLI; if it is absent, `auto` keeps whatever embedded text exists and `force` errors with an install hint. Rasterizing vector pages is out of scope — OCR targets the common "scanned page = one embedded image" case.

### Input formats (accepted by `build`)

| Extension | Notes |
|-----------|-------|
| `.xml` | Direct semantic XML — must conform to the V1 schema |
| `.html` | HTML5 — headings, tables, lists, code, figures extracted |
| `.md` | Markdown via `pulldown-cmark` — headings, paragraphs, ordered/unordered lists, GFM tables, fenced code (+language), blockquotes→citations, images→figures |
| `.typ` | Typst (line-based) — headings, lists, fenced code, `$…$` equations, `image()` figures |

### Export formats (via `export`)

| Format | Flag | Use |
|--------|------|-----|
| XML | `--format xml` | Raw semantic payload |
| Markdown | `--format markdown` | Human-readable rendering |
| Markdown AST | `--format markdown-ast` | MDAST-compatible JSON tree |
| ONTO | `--format onto` | Columnar token-efficient LLM ingestion |

### XML schema constraints (enforced by `xml::validate_xml`)

- Root element must be `<document version="MAJOR.MINOR">`.
- Version negotiation: any `1.x` is accepted (forward-compatible — unknown elements/attributes are ignored); other majors are rejected so the file degrades to a plain PDF. See `SUPPORTED_MAJOR_VERSION` and the "Versioning and Compatibility" section of `docs/spec.md`.
- Must contain at least one `<section>` with a stable `id` attribute.
- No DOCTYPE declarations or processing instructions.
- Sections must not be empty.

### Security invariants

- `sanitize_xml` runs on every XML path (both build and extract).
- Disallowed **active-content / structural** markers only: `<!DOCTYPE`, `<?xml-stylesheet`, `<?processing`, `<script`, `/JavaScript`, `/Launch` (the same list in the Rust core, Python SDK, and TS SDK). Body text is XML-escaped, so these match only real markup.
- Natural-language phrases (`system prompt`, `prompt:`, `model directive`) are intentionally **not** banned — XML text is data, not instructions, and the visible PDF already carries the same words. Banning them broke legitimate documents and PDF ingestion (e.g. `ingest` of a PDF discussing prompts); see `docs/security.md`.
- The semantic layer deliberately stores no embeddings, model output, or executable content.
- Decompressed payload capped at 16 MiB.

### ONTO export format

ONTO is a derived, export-only columnar format for LLM ingestion — it is never embedded in the PDF. The scalar encoding rules:
- Whitespace is normalized to single spaces.
- `|` → `/`, `^` → `;`, backtick → `'` (prevent delimiter collision).
- Strings containing newlines or leading/trailing spaces are backtick-wrapped.
- Table `rows` field is pre-serialized as `cell1^cell2|cell1^cell2` and emitted via `column_raw` (not re-encoded).

All three SDKs implement the same shape: `doc.to_onto()` (Python), `doc.toOnto()` (TypeScript), `AipdfDocument::to_onto()` (Rust). The MDAST exporter is likewise mirrored: `doc.to_markdown_ast()` (Python), `doc.toMarkdownAst()` (TypeScript), `xml_to_markdown_ast_json` (Rust core / CLI `export --format markdown-ast`).

### SDK layout

- `sdk/python/` — pure Python, depends on `brotli>=1.1.0`. `xml_to_onto` uses `xml.etree.ElementTree` with a recursive `walk`. The `_onto_scalar` encoder mirrors the Rust encoder exactly. `xml_to_markdown_ast_json` (method `doc.to_markdown_ast()`) mirrors the Rust AST walker via a recursive `_ast_emit`. `inspect_pdf` / `doc.inspect()` report the same byte counts as `aipdf inspect` (compressed = stream length, xml = sanitized-then-UTF-8-encoded length). Public class: `AIPDF`. Also ships `aipdf.mcp_server` (MCP stdio server; `aipdf-mcp` console script).
- `sdk/typescript/` — ESM TypeScript, no runtime deps. Uses Node's built-in `zlib` for Brotli. The read-side transforms (`xmlToMarkdown`/`xmlToMarkdownAstJson`/`xmlToOnto`/`getReadingOrder`/`collectElementText`) run on a small proper XML parser + DOM walk (`parseXml`), not regex. The AST builder (`mdNode`) emits node keys in the Rust field order and omits absent ones so `JSON.stringify` matches serde. `inspectPdf` / `doc.inspect()` mirror the Python/Rust byte counts. Public class: `AIPDF`.

**Write-side / image parity (both SDKs):** `build`, `ingest`, image extraction, and `bench` need the Rust core (PDF assembly, font embedding, lopdf, OCR, Brotli *compression*, raster re-encode), so the SDKs **delegate to the installed `aipdf` CLI binary** rather than reimplementing them — Python `aipdf.cli` (`build`/`ingest`/`export`/`extract_images`/`bench`), TS `buildPdf`/`ingest`/`exportSave`/`extractImages`/`bench` (`node:child_process`). `AIPDF_BIN` overrides the binary path (default `aipdf`). The native read path (`open` + `to_*` transforms + `inspect`/`validate`) needs no binary. This keeps the "Rust core is authoritative" invariant: SDKs never re-derive write-side bytes.

### Cross-SDK conformance (single source of truth)

The Rust core is authoritative. Golden ONTO/Markdown fixtures in `tests/conformance/` are generated from Rust and asserted byte-for-byte by all three implementations (`crates/aipdf/tests/conformance.rs`, `tests/conformance_python.py`, `sdk/typescript/test/conformance.test.mjs`). When changing any exporter, regenerate the goldens from Rust and confirm all three still match. The disallowed-marker lists in `security.rs`, the Python SDK, and the TS SDK are kept identical.

The `markdown-ast` exporter (MDAST-compatible JSON) is implemented in all three SDKs — `xml_to_markdown_ast_json` (Rust core + Python SDK), `xmlToMarkdownAstJson` (TS SDK) — and its golden (`tests/conformance/rich.ast.json`) is asserted by all three harnesses. Node objects are emitted in the Rust struct's field order (`type, value, depth, lang, ordered, url, alt, children`) with absent fields omitted, so the pretty JSON is byte-for-byte identical across `serde_json`, Python `json.dumps(ensure_ascii=False)`, and JS `JSON.stringify`. `rich.xml` carries a `<figure>`/`<image>`, so the AST golden also guards against the regression where self-closing `<image/>` nodes were dropped from the AST output.

### MCP server

`sdk/python/aipdf/mcp_server.py` is an MCP stdio server (newline-delimited JSON-RPC 2.0) whose tool surface mirrors the CLI one-for-one. **Read tools (native, no binary):** `aipdf_inspect` (now includes compressed/decompressed byte counts), `aipdf_extract` (`onto`/`markdown`/`markdown-ast`/`xml`), `aipdf_reading_order`, `aipdf_validate`. **Write tools (delegate to the `aipdf` CLI via `aipdf.cli`):** `aipdf_build`, `aipdf_extract_images` (`export --save`), `aipdf_convert` (`ingest`), `aipdf_bench`. Tool-level failures return `isError` results, not protocol errors. See `docs/mcp.md`.

### Samples and schema

- `samples/minimal.xml` / `samples/maximal.xml` — reference XML inputs used by tests (both valid v1.0; built `samples/*.pdf` are committed for SDK fixture tests).
- `tests/conformance/rich.xml` — v1.0 fixture exercising lists/tables/code/figures/equations for cross-SDK conformance.
- `crates/aipdf/assets/DejaVuSans.ttf` — bundled default embedded font (see `assets/FONT-LICENSE.md`).
- `schema/aipdf-1.0.xsd` — normative XSD schema for the semantic payload.
- `docs/spec.md` — canonical format specification (PDF object layout, detection order, metadata fields, versioning contract).
- `docs/security.md` — threat model and security controls.
- `docs/compatibility.md` — legacy PDF reader compatibility notes.
- `docs/mcp.md` — MCP server tools and client configuration.
