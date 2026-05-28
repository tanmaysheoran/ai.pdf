# aipdf

`aipdf` is a minimal, standards-aware prototype for an AI-native PDF extension.

An `.aipdf` file is still a fully valid PDF. It renders, prints, archives, and opens in ordinary PDF readers. The extension adds a compressed, declarative XML semantic layer as an associated embedded file so AI-aware parsers can read structure directly without OCR.

Core rule:

> Render once for humans. Structure once for machines. Duplicate nothing.

## What V1 Includes

- File format specification in [docs/spec.md](docs/spec.md)
- XML schema in [schema/aipdf-1.0.xsd](schema/aipdf-1.0.xsd)
- Rust core crate in [crates/aipdf](crates/aipdf)
- Rust CLI in [crates/aipdf-cli](crates/aipdf-cli)
- Python SDK in [sdk/python](sdk/python)
- TypeScript SDK in [sdk/typescript](sdk/typescript)
- Samples in [samples](samples)
- Benchmarks and analysis in [benches](benches)
- Compatibility and security docs in [docs](docs)

## Format Summary

The PDF contains:

- A normal page tree and content streams for visual rendering.
- XMP metadata advertising `aipdf:Version` and semantic payload metadata.
- A PDF Associated File entry (`/AF`) pointing to an embedded file named `aipdf-semantic.xml.br`.
- The embedded file bytes are Brotli-compressed XML.
- The XML validates against the V1 schema and is declarative only.

Legacy PDF readers ignore the associated file and render the PDF normally.

## CLI

```bash
aipdf build samples/minimal.xml -o samples/minimal.aipdf
aipdf inspect samples/minimal.aipdf
aipdf validate samples/minimal.aipdf
aipdf extract samples/minimal.aipdf
aipdf export samples/minimal.aipdf --format markdown
aipdf export samples/minimal.aipdf --format markdown-ast
```

## Local Verification

```bash
python3 -m venv .venv
.venv/bin/python -m pip install -e sdk/python
.venv/bin/python tests/python_smoke.py

cd sdk/typescript
npm install
npm run build
npm test
```

Rust verification requires Cargo:

```bash
cargo test
cargo run -p aipdf-cli -- build samples/minimal.xml -o samples/minimal.aipdf
```

## SDK Examples

Python:

```python
from aipdf import AIPDF

doc = AIPDF.open("samples/minimal.aipdf")
print(doc.to_xml())
print(doc.to_markdown())
```

TypeScript:

```ts
import { AIPDF } from "@aipdf/sdk";

const doc = AIPDF.open("samples/minimal.aipdf");
console.log(doc.toMarkdown());
```

Rust:

```rust
let doc = aipdf::AipdfDocument::open("samples/minimal.aipdf")?;
println!("{}", doc.to_markdown()?);
```

## Prototype Boundaries

This repository deliberately does not store embeddings, prompts, hidden instructions, summaries, model output, code, notebook runtimes, or vector databases. The semantic layer is a compact, compressed, schema-validated XML document.
