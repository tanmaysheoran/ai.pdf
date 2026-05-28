# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Project Is

`aipdf` is a prototype AI-native PDF extension format. An `.aipdf` file is a valid PDF that embeds a Brotli-compressed semantic XML layer as an associated file (`aipdf-semantic.xml.br`). Legacy PDF readers ignore it; AI-aware parsers extract structure directly without OCR.

The invariant: **PDF is the visual authority. The embedded XML is the machine-structure authority. Nothing is duplicated.**

## Commands

### Rust (core library + CLI)
```bash
cargo test                    # run all Rust tests
cargo test -p aipdf           # test core library only
cargo run -p aipdf-cli -- build samples/minimal.xml -o samples/minimal.aipdf
cargo run -p aipdf-cli -- inspect samples/minimal.aipdf
cargo run -p aipdf-cli -- validate samples/minimal.aipdf
cargo run -p aipdf-cli -- extract samples/minimal.aipdf
cargo run -p aipdf-cli -- export samples/minimal.pdf --format markdown
cargo run -p aipdf-cli -- export samples/minimal.pdf --format onto
cargo run -p aipdf-cli -- export samples/maximal.pdf --format onto
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

## Architecture

The workspace has two crates:

- **`crates/aipdf`** — core library. All format logic lives here.
- **`crates/aipdf-cli`** — thin CLI wrapper using `clap` that calls into `crates/aipdf`.

### Core library modules (`crates/aipdf/src/`)

| Module | Responsibility |
|--------|---------------|
| `lib.rs` | Public API: `AipdfDocument` struct, re-exports, `AipdfError` |
| `pdf.rs` | PDF byte assembly (`build_aipdf`), Brotli compress/decompress, `extract_semantic_xml`, `inspect_pdf`. Writes a minimal hand-crafted PDF 1.7 with 9 objects. |
| `xml.rs` | XML validation (structural rules, no DOCTYPE/PI), `get_reading_order` → `Vec<SemanticBlock>`, `get_tables`, `find_citations` |
| `markdown.rs` | `xml_to_markdown` (rendered string) and `xml_to_markdown_ast_json` (MDAST-compatible JSON) |
| `onto.rs` | `xml_to_onto` — columnar ONTO-style export for token-efficient LLM ingestion. Emits `Document`, `Sections`, `Blocks`, `Tables`, `Figures`, `References` in source order. Table rows use `^` (cell separator) and `|` (row separator); pre-serialized rows are joined by `column_raw` without re-encoding. |
| `security.rs` | `sanitize_xml` — rejects disallowed markers (DOCTYPE, JS, prompt/model-directive strings), enforces 16 MiB size cap |
| `source.rs` | `semantic_xml_from_source` — converts Markdown, HTML, Typst, or raw XML inputs into valid semantic XML |

### Data flow

```
Input (XML/MD/HTML/Typst)
  → source::semantic_xml_from_source
  → security::sanitize_xml
  → xml::validate_xml
  → pdf::build_aipdf   (Brotli compress → embed in hand-written PDF bytes)
  → .aipdf file

.aipdf file
  → pdf::extract_semantic_xml   (find stream by subtype marker → Brotli decompress → sanitize + validate)
  → markdown::xml_to_markdown / onto::xml_to_onto / xml::get_reading_order
```

### XML schema constraints (enforced by `xml::validate_xml`)

- Root element must be `<document version="1.0">`.
- Must contain at least one `<section>` with a stable `id` attribute.
- No DOCTYPE declarations or processing instructions.
- Sections must not be empty.

### Security invariants

- `sanitize_xml` runs on every XML path (both build and extract).
- Disallowed strings: `<!DOCTYPE`, `<?xml-stylesheet`, `<script`, `/JavaScript`, `/Launch`, `prompt:`, `system prompt`, `model directive`.
- The semantic layer deliberately stores no embeddings, prompts, model output, or executable content.

### ONTO export format

ONTO is a derived, export-only columnar format for LLM ingestion — it is never embedded in the PDF. The scalar encoding rules:
- Whitespace is normalized to single spaces.
- `|` → `/`, `^` → `;`, backtick → `'` (prevent delimiter collision).
- Strings containing newlines or leading/trailing spaces are backtick-wrapped.
- Table `rows` field is pre-serialized as `cell1^cell2|cell1^cell2` and emitted via `column_raw` (not re-encoded).

All three SDKs implement the same shape: `doc.to_onto()` (Python), `doc.toOnto()` (TypeScript), `AipdfDocument::to_onto()` (Rust).

### SDK layout

- `sdk/python/` — pure Python, depends on `brotli>=1.1.0`. `xml_to_onto` uses `xml.etree.ElementTree` with a recursive `walk`. The `_onto_scalar` encoder mirrors the Rust encoder exactly.
- `sdk/typescript/` — ESM TypeScript, no runtime deps. Uses Node's built-in `zlib` for Brotli. `xmlToOnto` uses regex-based streaming (same approach as `xmlToMarkdown`).

### Samples and schema

- `samples/minimal.xml` / `samples/maximal.xml` — reference XML inputs used by tests.
- `schema/aipdf-1.0.xsd` — normative XSD schema for the semantic payload.
- `docs/spec.md` — canonical format specification (PDF object layout, detection order, metadata fields).
