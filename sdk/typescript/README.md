# @aipdf/sdk

TypeScript SDK for reading and writing `.ai.pdf` files — valid PDFs with an embedded Brotli-compressed semantic XML layer.

The **read path** is pure TypeScript (no binary needed). The **write path** (`buildPdf`, `ingest`, image extraction) delegates to the `aipdf` Rust CLI.

## Installation

```bash
npm install @aipdf/sdk
```

No runtime dependencies. Requires Node.js ≥ 18 (uses built-in `zlib` for Brotli).

## Quick start

```ts
import { AIPDF } from "@aipdf/sdk";

const doc = AIPDF.open("report.ai.pdf");

if (doc.hasSemanticLayer) {
  console.log(doc.toMarkdown());
}
```

## Read API (no binary required)

### Opening a file

```ts
import { AIPDF, AIPDFDocument } from "@aipdf/sdk";

const doc: AIPDFDocument = AIPDF.open("report.ai.pdf");
```

### `AIPDFDocument` properties and methods

| Member | Returns | Description |
|--------|---------|-------------|
| `doc.hasSemanticLayer` | `boolean` | Whether the file has an embedded semantic layer |
| `doc.path` | `string \| undefined` | Backing file path |
| `doc.xml` | `string \| undefined` | Raw semantic XML (if present) |
| `doc.inspect()` | `InspectReport` | Byte-level report (compressed size, XML size) |
| `doc.validate()` | `boolean` | Validate the embedded XML; throws `AIPDFError` on failure |
| `doc.toXml()` | `string` | Raw semantic XML string |
| `doc.toMarkdown()` | `string` | Markdown rendering of the document |
| `doc.toMarkdownAst()` | `string` | MDAST-compatible JSON tree |
| `doc.toOnto()` | `string` | Columnar ONTO format for token-efficient LLM ingestion |
| `doc.getStructure()` | `SemanticBlock[]` | Blocks in reading order |
| `doc.getReadingOrder()` | `SemanticBlock[]` | Alias for `getStructure()` |
| `doc.getTables()` | `string[]` | Text content of all `<table>` elements |
| `doc.findCitations()` | `string[]` | Text content of all `<citation>` elements |

### `SemanticBlock`

```ts
interface SemanticBlock {
  kind: string;        // element tag: "heading", "paragraph", "table", …
  id?: string;         // stable section/block id
  page?: number;       // page number (set by --render full)
  bbox?: string;       // bounding box string (set by --render full)
  text: string;        // plain-text content
}
```

### `InspectReport`

```ts
interface InspectReport {
  isPdf: boolean;
  hasSemanticLayer: boolean;
  semanticCompressedBytes?: number;
  semanticXmlBytes?: number;
}
```

## Write API (requires `aipdf` CLI)

These functions shell out to the `aipdf` binary.

**Install the CLI** (one-liner):

```bash
curl -fsSL https://aipdf.arp244.dev/install.sh | sh
```

Or from a cloned repo:

```bash
./install.sh          # or: cargo install --path crates/aipdf-cli
```

Point to a custom binary via the `AIPDF_BIN` environment variable (default: `aipdf`).

### `buildPdf` — create a `.ai.pdf` from source

```ts
import { buildPdf } from "@aipdf/sdk";

// From Markdown, HTML, XML, or Typst
const outPath = buildPdf("paper.md", { output: "paper.ai.pdf", render: "full", pageSize: "letter" });
const outPath = buildPdf("page.html", { render: "browser" });   // full CSS via headless Chrome
const outPath = buildPdf("doc.md", { font: "/path/to/NotoSansCJK.ttf" });
```

```ts
interface BuildOptions {
  output?: string;
  render?: "minimal" | "full" | "browser";  // default: "minimal"
  pageSize?: "letter" | "a4";               // default: "letter"
  font?: string;                            // path to a .ttf file
  title?: string;
}
```

### `ingest` — attach a semantic layer to an existing PDF

```ts
import { ingest } from "@aipdf/sdk";

const outPath = ingest("scanned.pdf", { output: "scanned.ai.pdf" });
const outPath = ingest("report.pdf", { ocr: "never" });
const outPath = ingest("foreign.pdf", { lang: "eng+deu" });
```

```ts
interface IngestOptions {
  output?: string;
  ocr?: "auto" | "never" | "force";  // default: "auto"
  lang?: string;                     // tesseract language code(s), default: "eng"
}
```

### `exportSave` — export the semantic layer to disk

```ts
import { exportSave } from "@aipdf/sdk";

const result = exportSave("doc.ai.pdf", "/out", "markdown");
// result.output  — path to the content file
// result.images  — paths to any extracted raster images
```

### `extractImages` — pull embedded raster images

```ts
import { extractImages } from "@aipdf/sdk";

const imagePaths: string[] = extractImages("doc.ai.pdf", "/out/images");
```

### `bench` — timing report

```ts
import { bench } from "@aipdf/sdk";

const timings: Record<string, string> = bench("doc.ai.pdf");
```

## Low-level helpers

Module-level functions exported for direct use:

```ts
import {
  xmlToMarkdown,          // (xml: string) => string
  xmlToMarkdownAstJson,   // (xml: string) => string  (JSON)
  xmlToOnto,              // (xml: string) => string
  getReadingOrder,        // (xml: string) => SemanticBlock[]
  collectElementText,     // (xml: string, tag: string) => string[]
  inspectPdf,             // (data: Buffer) => InspectReport
  extractSemanticXml,     // (data: Buffer) => string | undefined
  findSemanticStream,     // (data: Buffer) => Buffer | undefined
  sanitizeXml,            // (xml: string) => string
  validateXml,            // (xml: string) => void  (throws on invalid)
} from "@aipdf/sdk";
```

## Error handling

All errors throw `AIPDFError`:

```ts
import { AIPDF, AIPDFError } from "@aipdf/sdk";

try {
  const doc = AIPDF.open("file.pdf");
  console.log(doc.toMarkdown());
} catch (e) {
  if (e instanceof AIPDFError) console.error("aipdf error:", e.message);
  else throw e;
}
```

## Build from source

```bash
cd sdk/typescript
npm install
npm run build
npm test
```
