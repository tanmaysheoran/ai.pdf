# aipdf MCP server

The `aipdf` Python package ships a small [Model Context
Protocol](https://modelcontextprotocol.io) server so AI agents can read the
semantic layer of an `.ai.pdf` directly — no OCR, no layout heuristics, just the
embedded machine-structure authority.

## What it exposes

The tool surface mirrors the `aipdf` CLI one-for-one. Read tools run natively in
the Python package; write tools (`aipdf_build`, `aipdf_convert`,
`aipdf_extract_images`, `aipdf_bench`) delegate to the installed `aipdf` binary
(set `AIPDF_BIN` to override its path).

| Tool | CLI equivalent | Purpose |
|------|----------------|---------|
| `aipdf_inspect` | `inspect` | Report whether a file is a PDF, whether it carries an `.ai.pdf` semantic layer, and the compressed / decompressed semantic-layer byte sizes. |
| `aipdf_extract` | `extract` / `export` | Return the semantic layer as `onto` (token-efficient columnar, default), `markdown`, `markdown-ast` (MDAST JSON), or `xml`. |
| `aipdf_reading_order` | — | Return the document's semantic blocks in reading order as JSON (`kind`, `id`, `page`, `bbox`, `text`). |
| `aipdf_validate` | `validate` | Validate the embedded semantic XML against the V1 schema constraints. |
| `aipdf_build` | `build` | Build an `.ai.pdf` from a source file (`render` = `minimal` / `full` / `browser`, `page_size`, `font`, `title`). **Requires the CLI binary.** |
| `aipdf_extract_images` | `export --save` | Extract embedded raster images to a directory alongside the rendered content file. **Requires the CLI binary.** |
| `aipdf_convert` | `ingest` | Attach a semantic layer to a plain PDF via text extraction (with optional OCR). **Requires the CLI binary.** |
| `aipdf_bench` | `bench` | Report the source XML and `.ai.pdf` byte sizes. **Requires the CLI binary.** |

The server speaks newline-delimited JSON-RPC 2.0 over stdio (the MCP stdio
transport) and is dependency-free beyond the `aipdf` package itself for the read
tools; the write tools shell out to the `aipdf` CLI.

## Install and run

```bash
python3 -m pip install -e sdk/python   # provides the `aipdf-mcp` command
aipdf-mcp                               # or: python -m aipdf.mcp_server
```

## Client configuration

Claude Desktop / Claude Code (`mcp` config), for example:

```json
{
  "mcpServers": {
    "aipdf": {
      "command": "aipdf-mcp"
    }
  }
}
```

Then an agent can do, e.g.:

> "Inspect `report.ai.pdf` and summarise it."

The agent calls `aipdf_inspect` to confirm the semantic layer is present, then
`aipdf_extract` (format `onto`) to ingest the structure compactly. Because the
structure is authored, not guessed, the agent reads tables, figures, citations,
and reading order exactly as the document declares them.

## Why ONTO by default

`onto` is a columnar, delimiter-packed projection designed for token-efficient
LLM ingestion: one block per column family, table rows pre-serialised, scalars
normalised. It conveys the same structure as the XML at a fraction of the
tokens. Use `markdown` when you want a human-readable rendering, or `xml` for the
raw payload.
```
