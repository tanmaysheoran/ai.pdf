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
import subprocess
import sys
from pathlib import Path
from typing import Any

from .core import AIPDF, AIPDFError, get_reading_order

PROTOCOL_VERSION = "2024-11-05"
SERVER_INFO = {"name": "aipdf", "version": "0.1.0"}

TOOLS = [
    {
        "name": "aipdf_inspect",
        "description": "Check whether a file is a PDF and whether it carries an .ai.pdf semantic layer.",
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
            "format=onto (token-efficient columnar, default), markdown, or xml."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to the .ai.pdf file."},
                "format": {"type": "string", "enum": ["onto", "markdown", "xml"], "default": "onto"},
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
            doc = AIPDF.open(path)
            return json.dumps(
                {"path": path, "is_pdf": doc.is_pdf, "has_semantic_layer": doc.has_semantic_layer},
            )
        except FileNotFoundError:
            raise AIPDFError(f"file not found: {path}")
    if name == "aipdf_extract":
        doc = _open(args["path"])
        fmt = args.get("format", "onto")
        if fmt == "onto":
            return doc.to_onto()
        if fmt == "markdown":
            return doc.to_markdown()
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
    if name == "aipdf_convert":
        input_path = args["path"]
        if not Path(input_path).exists():
            raise AIPDFError(f"file not found: {input_path}")
        output_path = args.get("output") or str(Path(input_path).with_suffix(".ai.pdf"))
        cmd = ["aipdf", "ingest", input_path, "-o", output_path]
        ocr = args.get("ocr", "auto")
        if ocr:
            cmd += ["--ocr", ocr]
        lang = args.get("lang")
        if lang:
            cmd += ["--lang", lang]
        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
        except FileNotFoundError:
            raise AIPDFError(
                "aipdf CLI not found; install it from https://github.com/aiPDF/aipdf or add it to PATH"
            )
        if result.returncode != 0:
            raise AIPDFError(result.stderr.strip() or f"aipdf ingest failed (exit {result.returncode})")
        return json.dumps({"output": output_path, "message": result.stdout.strip() or "conversion successful"})
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
