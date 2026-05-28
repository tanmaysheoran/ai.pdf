# PDF Compatibility Tests

V1 compatibility expectations:

- Ordinary PDF readers render the visible page content.
- Print pipelines ignore `/AF` and embedded semantic files.
- Browsers open the file as a PDF when served as `application/pdf`.
- Archival tools preserve embedded files and XMP metadata.
- AI-aware parsers can extract `aipdf-semantic.xml.br` without OCR.

## Manual Test Matrix

| Reader | Expected result |
| --- | --- |
| Chrome PDF viewer | Opens and renders visible text |
| Firefox PDF viewer | Opens and renders visible text |
| macOS Preview | Opens and renders visible text |
| Adobe Acrobat Reader | Opens and shows attachment metadata |
| Poppler `pdfinfo` | Reports a valid PDF |

## Automated Prototype Tests

The repository includes parser-level tests for:

- `%PDF-` header preservation
- `%%EOF` trailer preservation
- embedded semantic stream detection
- Brotli decompression
- Markdown reconstruction
- ordinary-PDF fallback

