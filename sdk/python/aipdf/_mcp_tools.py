"""PROTOCOL_VERSION, SERVER_INFO, and the TOOLS list for the aipdf MCP server."""
from __future__ import annotations

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
