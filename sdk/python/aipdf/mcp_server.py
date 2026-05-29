"""A minimal Model Context Protocol (MCP) server for ``.ai.pdf`` files.

It lets an MCP-aware agent (Claude Desktop, Claude Code, etc.) detect the
semantic layer of an ``.ai.pdf`` and pull its machine-readable structure
(ONTO / Markdown / XML / reading order) directly — no OCR, no heuristics.

Transport: newline-delimited JSON-RPC 2.0 over stdio (the MCP stdio transport).
Deliberately dependency-free — it speaks the protocol directly rather than
pulling in the MCP SDK, so it runs anywhere the ``aipdf`` package is installed.

Run it as::

    python -m aipdf.mcp_server

and point your MCP client at that command.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

from . import cli
from .core import AIPDF, AIPDFError, get_reading_order, inspect_pdf

PROTOCOL_VERSION = "2024-11-05"
SERVER_INFO = {"name": "aipdf", "version": "0.1.0"}

TOOLS = [
    {
        "name": "aipdf_inspect",
        "description": (
            "Check whether a file is a PDF and whether it carries an .ai.pdf semantic layer, "
            "reporting the compressed and decompressed semantic-layer byte sizes."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {"path": {"type": "string", "description": "Path to the PDF / .ai.pdf file."}},
            "required": ["path"],
        },
    },
    {
        "name": "aipdf_extract",
        "description": (
            "Extract the semantic layer of an .ai.pdf for LLM consumption. "
            "format=onto (token-efficient columnar, default), markdown, markdown-ast (MDAST JSON), or xml."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to the .ai.pdf file."},
                "format": {
                    "type": "string",
                    "enum": ["onto", "markdown", "markdown-ast", "xml"],
                    "default": "onto",
                },
            },
            "required": ["path"],
        },
    },
    {
        "name": "aipdf_reading_order",
        "description": "Return the document's semantic blocks in reading order as JSON (kind, id, page, bbox, text).",
        "inputSchema": {
            "type": "object",
            "properties": {"path": {"type": "string", "description": "Path to the .ai.pdf file."}},
            "required": ["path"],
        },
    },
    {
        "name": "aipdf_validate",
        "description": "Validate the embedded semantic XML of an .ai.pdf against the V1 schema constraints.",
        "inputSchema": {
            "type": "object",
            "properties": {"path": {"type": "string", "description": "Path to the .ai.pdf file."}},
            "required": ["path"],
        },
    },
    {
        "name": "aipdf_build",
        "description": (
            "Build an .ai.pdf from a source file (.xml/.md/.html/.typ). render=minimal (plain text, default), "
            "full (laid-out PDF), or browser (full CSS via headless Chrome, HTML input only). "
            "Requires the aipdf CLI to be installed."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to the source file (.xml/.md/.html/.typ)."},
                "output": {"type": "string", "description": "Output .ai.pdf path. Defaults to <input>.ai.pdf."},
                "render": {"type": "string", "enum": ["minimal", "full", "browser"], "default": "minimal"},
                "page_size": {"type": "string", "enum": ["letter", "a4"], "default": "letter"},
                "font": {"type": "string", "description": "Path to a TrueType font to embed (e.g. a Noto CJK face)."},
                "title": {"type": "string", "description": "Document title."},
            },
            "required": ["path"],
        },
    },
    {
        "name": "aipdf_extract_images",
        "description": (
            "Extract embedded raster images from an .ai.pdf to a directory, alongside the rendered "
            "content file. Returns the saved file paths. Requires the aipdf CLI to be installed."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to the .ai.pdf file."},
                "out_dir": {"type": "string", "description": "Directory to write the content file and images to."},
                "format": {
                    "type": "string",
                    "enum": ["xml", "markdown", "markdown-ast", "onto"],
                    "default": "markdown",
                    "description": "Format of the rendered content file written alongside the images.",
                },
            },
            "required": ["path", "out_dir"],
        },
    },
    {
        "name": "aipdf_convert",
        "description": (
            "Convert a plain PDF to an .ai.pdf by attaching a semantic layer via text extraction "
            "(with optional OCR for scanned pages). Requires the aipdf CLI to be installed."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to the input PDF file."},
                "output": {
                    "type": "string",
                    "description": "Output path for the .ai.pdf file. Defaults to the input path with .ai.pdf extension.",
                },
                "ocr": {
                    "type": "string",
                    "enum": ["auto", "never", "force"],
                    "default": "auto",
                    "description": "OCR mode: auto (OCR low-text pages), never, or force.",
                },
                "lang": {
                    "type": "string",
                    "description": "Tesseract language code(s) for OCR, e.g. 'eng' or 'eng+deu'. Default: eng.",
                },
            },
            "required": ["path"],
        },
    },
    {
        "name": "aipdf_bench",
        "description": "Report the XML and .ai.pdf byte sizes for a source file. Requires the aipdf CLI.",
        "inputSchema": {
            "type": "object",
            "properties": {"path": {"type": "string", "description": "Path to the source file (.xml/.md/.html/.typ)."}},
            "required": ["path"],
        },
    },
]


def _open(path: str) -> AIPDF:
    try:
        doc = AIPDF.open(path)
    except FileNotFoundError:
        raise AIPDFError(f"file not found: {path}")
    if not doc.has_semantic_layer:
        raise AIPDFError(f"no .ai.pdf semantic layer found in {path}")
    return doc


def call_tool(name: str, args: dict[str, Any]) -> str:
    """Dispatch a tool call and return its text result."""
    if name == "aipdf_inspect":
        path = args["path"]
        try:
            report = inspect_pdf(Path(path).read_bytes())
        except FileNotFoundError:
            raise AIPDFError(f"file not found: {path}")
        return json.dumps(
            {
                "path": path,
                "is_pdf": report.is_pdf,
                "has_semantic_layer": report.has_semantic_layer,
                "semantic_compressed_bytes": report.semantic_compressed_bytes,
                "semantic_xml_bytes": report.semantic_xml_bytes,
            },
        )
    if name == "aipdf_extract":
        doc = _open(args["path"])
        fmt = args.get("format", "onto")
        if fmt == "onto":
            return doc.to_onto()
        if fmt == "markdown":
            return doc.to_markdown()
        if fmt == "markdown-ast":
            return doc.to_markdown_ast()
        if fmt == "xml":
            return doc.to_xml()
        raise AIPDFError(f"unknown format: {fmt}")
    if name == "aipdf_reading_order":
        doc = _open(args["path"])
        blocks = get_reading_order(doc.to_xml())
        return json.dumps(
            [{"kind": b.kind, "id": b.id, "page": b.page, "bbox": b.bbox, "text": b.text} for b in blocks],
            indent=2,
        )
    if name == "aipdf_validate":
        doc = _open(args["path"])
        doc.validate()
        return json.dumps({"path": args["path"], "valid": True})
    if name == "aipdf_build":
        path = args["path"]
        if not Path(path).exists():
            raise AIPDFError(f"file not found: {path}")
        out = cli.build(
            path,
            args.get("output"),
            render=args.get("render", "minimal"),
            page_size=args.get("page_size", "letter"),
            font=args.get("font"),
            title=args.get("title"),
        )
        return json.dumps({"output": str(out), "message": "build successful"})
    if name == "aipdf_extract_images":
        path = args["path"]
        if not Path(path).exists():
            raise AIPDFError(f"file not found: {path}")
        result = cli.export(path, args.get("format", "markdown"), args["out_dir"])
        return json.dumps(
            {"output": str(result.output), "images": [str(p) for p in result.images]},
        )
    if name == "aipdf_convert":
        input_path = args["path"]
        if not Path(input_path).exists():
            raise AIPDFError(f"file not found: {input_path}")
        out = cli.ingest(
            input_path,
            args.get("output"),
            ocr=args.get("ocr", "auto"),
            lang=args.get("lang", "eng"),
        )
        return json.dumps({"output": str(out), "message": "conversion successful"})
    if name == "aipdf_bench":
        path = args["path"]
        if not Path(path).exists():
            raise AIPDFError(f"file not found: {path}")
        return json.dumps(cli.bench(path))
    raise AIPDFError(f"unknown tool: {name}")


def _result(req_id: Any, result: dict[str, Any]) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": req_id, "result": result}


def _error(req_id: Any, code: int, message: str) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": req_id, "error": {"code": code, "message": message}}


def handle(message: dict[str, Any]) -> dict[str, Any] | None:
    """Handle one JSON-RPC message; return a response, or None for notifications."""
    method = message.get("method")
    req_id = message.get("id")

    if method == "initialize":
        return _result(
            req_id,
            {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {}},
                "serverInfo": SERVER_INFO,
            },
        )
    if method in ("notifications/initialized", "initialized"):
        return None  # notification, no response
    if method == "tools/list":
        return _result(req_id, {"tools": TOOLS})
    if method == "tools/call":
        params = message.get("params") or {}
        name = params.get("name", "")
        args = params.get("arguments") or {}
        try:
            text = call_tool(name, args)
            return _result(req_id, {"content": [{"type": "text", "text": text}]})
        except AIPDFError as exc:
            # Tool-level errors are reported as result with isError so the model sees them.
            return _result(req_id, {"content": [{"type": "text", "text": str(exc)}], "isError": True})
        except Exception as exc:  # pragma: no cover - defensive
            return _error(req_id, -32603, f"internal error: {exc}")
    if req_id is not None:
        return _error(req_id, -32601, f"method not found: {method}")
    return None


def serve(stdin=None, stdout=None) -> None:
    """Run the stdio JSON-RPC loop until EOF."""
    stdin = stdin or sys.stdin
    stdout = stdout or sys.stdout
    for line in stdin:
        line = line.strip()
        if not line:
            continue
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        response = handle(message)
        if response is not None:
            stdout.write(json.dumps(response) + "\n")
            stdout.flush()


if __name__ == "__main__":
    serve()
