# aipdf MCP server

The `aipdf` Python package ships a small [Model Context
Protocol](https://modelcontextprotocol.io) server so AI agents can read the
semantic layer of an `.ai.pdf` directly — no OCR, no layout heuristics, just the
embedded machine-structure authority.

## What it exposes

| Tool | Purpose |
|------|---------|
| `aipdf_inspect` | Report whether a file is a PDF and whether it carries an `.ai.pdf` semantic layer. |
| `aipdf_extract` | Return the semantic layer as `onto` (token-efficient columnar, default), `markdown`, or `xml`. |
| `aipdf_reading_order` | Return the document's semantic blocks in reading order as JSON (`kind`, `id`, `page`, `bbox`, `text`). |

The server speaks newline-delimited JSON-RPC 2.0 over stdio (the MCP stdio
transport) and is dependency-free beyond the `aipdf` package itself.

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
