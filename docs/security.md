# Security Analysis

The `.aipdf` semantic layer is data, not behavior.

## Threat Model

Attackers may try to hide instructions, executable content, oversized payloads, entity expansion attacks, path traversal names, or ambiguous structure inside a PDF that still renders normally.

## Controls

- The semantic payload is XML validated against `aipdf-1.0.xsd`.
- Parsers reject `DOCTYPE` and processing instructions.
- Parsers never resolve external entities.
- `sanitize_xml` rejects active-content / structural markers as **markup**:
  `<!DOCTYPE`, `<?xml-stylesheet`, `<?processing`, `<script`, `/JavaScript`,
  `/Launch`. Because body text is XML-escaped, these only trip on real markup,
  not on prose that happens to mention them.
- The embedded file name is fixed to `aipdf-semantic.xml.br`.
- The embedded MIME marker is fixed to `application/aipdf+xml+br`.
- XML text is **data, not instructions**. Natural-language phrases (e.g.
  "system prompt", "prompt:", "model directive") are *not* banned: the visible
  PDF layer already carries the same words, and banning them would reject
  legitimate documents (AI papers, or ingesting any PDF that discusses
  prompting) for no security benefit. Consumers must treat the semantic layer
  as data and never execute it as instructions.
- No JavaScript, launch actions, multimedia actions, or executable attachments are part of the V1 profile.
- Implementations should cap decompressed XML size. The Rust prototype defaults to 16 MiB.

## Semantic Integrity Checks

Validation should ensure:

- required stable IDs exist on sections
- page numbers are positive integers
- bounding boxes parse as four finite numbers
- coordinates are page-local
- table rows contain cells
- semantic roles are from the V1 vocabulary

## Residual Risk

PDF itself is a large legacy format. This prototype uses ordinary embedded files and metadata, but production consumers should still process untrusted PDFs in a sandboxed parser and avoid executing any PDF active content.

