# aipdf V1 File Format Specification

Status: prototype, open for implementation feedback.

## Goals

`.aipdf` is a PDF-compatible extension that embeds a compact semantic XML layer inside an ordinary PDF. The PDF remains the visual rendering authority. The XML is the machine-structure authority.

V1 is intentionally small:

- preserve semantic structure
- preserve reading order
- map semantic blocks to PDF pages and coordinates
- support deterministic extraction
- avoid executable or generative payloads

## Container

An `.aipdf` file is a PDF 1.7 or PDF 2.0 compatible file with a different extension.

Required PDF-level objects:

- Catalog with `/Names` entry for embedded files.
- Catalog with `/AF` array containing the semantic embedded file spec.
- Embedded file stream containing Brotli-compressed semantic XML.
- File specification dictionary with `/AFRelationship /Data`.
- XMP metadata packet declaring the semantic layer.
- Optional `/Info` dictionary note for legacy metadata readers.

Recommended embedded filename:

```text
aipdf-semantic.xml.br
```

Recommended MIME marker:

```text
application/aipdf+xml+br
```

PDF name encoding uses `#` escapes:

```pdf
/Subtype /application#aipdf+xml+br
```

## PDF Object Map

Minimal object layout:

```text
1 0 obj  Catalog
2 0 obj  Pages
3 0 obj  Page
4 0 obj  Font
5 0 obj  Visible content stream
6 0 obj  XMP metadata stream
7 0 obj  EmbeddedFiles names dictionary
8 0 obj  FileSpec for aipdf-semantic.xml.br
9 0 obj  EmbeddedFile stream, Brotli XML bytes
```

The catalog links both metadata and associated files:

```pdf
<<
  /Type /Catalog
  /Pages 2 0 R
  /Metadata 6 0 R
  /Names << /EmbeddedFiles 7 0 R >>
  /AF [8 0 R]
>>
```

## Semantic Payload

The embedded payload is a Brotli-compressed XML document conforming to `schema/aipdf-1.0.xsd`.

Root:

```xml
<document version="1.0" id="doc1" lang="en">
  ...
</document>
```

Stable IDs are required for semantic blocks that may be referenced externally or mapped to PDF coordinates.

Coordinates use PDF user-space points. Bounding boxes are represented as:

```text
x0,y0,x1,y1
```

Coordinates are page-local and deterministic. The page number is 1-based.

## Compression

V1 requires Brotli for the semantic XML payload. The stream is stored as the actual embedded file bytes rather than a PDF filter because Brotli is not a standard PDF stream filter. Parsers identify it by filename, subtype, and XMP metadata, then decompress externally.

Recommended Brotli quality:

```text
quality = 6
lgwin = 22
```

This keeps compression strong while avoiding slow archival-level settings.

## Detection

AI-aware parsers should use this order:

1. Read the catalog `/AF` array and resolve file specs with `/AFRelationship /Data`.
2. Check embedded filenames for `aipdf-semantic.xml.br`.
3. Check embedded stream subtype for `application/aipdf+xml+br`.
4. Fall back to `/Names /EmbeddedFiles` if `/AF` is missing.
5. Fall back to ordinary PDF handling if no semantic layer exists.

## Metadata Fields

XMP is the authoritative discovery metadata location. V1 uses declarative fields rather than imperative AI instructions:

```xml
<rdf:Description rdf:about=""
  xmlns:aipdf="https://aipdf.org/ns/1.0/">
  <aipdf:Version>1.0</aipdf:Version>
  <aipdf:SemanticFile>aipdf-semantic.xml.br</aipdf:SemanticFile>
  <aipdf:SemanticEncoding>brotli</aipdf:SemanticEncoding>
  <aipdf:SemanticLayerPresent>true</aipdf:SemanticLayerPresent>
  <aipdf:SemanticMimeType>application/aipdf+xml+br</aipdf:SemanticMimeType>
  <aipdf:ContentAuthority>semantic-xml</aipdf:ContentAuthority>
  <aipdf:VisibleRenderingAuthority>pdf-page-content</aipdf:VisibleRenderingAuthority>
  <aipdf:OCRPolicy>skip-when-semantic-layer-present</aipdf:OCRPolicy>
</rdf:Description>
```

The older PDF `/Info` dictionary may include a flat `/AIPDFNote` for legacy tooling:

```pdf
/AIPDFNote (AIPDF semantic layer present: extract aipdf-semantic.xml.br, Brotli-decompress, parse XML.)
```

V1 intentionally avoids a field named `instructions` because that can be confused with hidden prompt or model-directive content. Parsers should treat these fields as discovery metadata only.

## Versioning and Compatibility

The semantic payload is versioned by the root attribute
`<document version="MAJOR.MINOR">`. Version negotiation follows three rules:

1. **Major version is the compatibility boundary.** A reader implements one
   major version (this prototype: `1`). It MUST accept any payload whose major
   matches, regardless of minor (`1.0`, `1.3`, `1.99` are all accepted by a v1
   reader).
2. **Minor versions are forward-compatible and additive.** New minors may add
   elements or attributes but never repurpose or remove existing ones. A reader
   MUST ignore elements and attributes it does not recognise rather than fail.
   This lets a v1.0 reader open a v1.3 file, simply skipping the v1.3 additions.
3. **A different major version is rejected.** When a reader encounters a major
   version it does not implement, it MUST NOT attempt to interpret the semantic
   layer. It falls back to treating the file as an ordinary PDF (the visual
   layer is always valid) and SHOULD surface a "newer semantic format" notice.
   `validate_xml` enforces this: a non-`1.x` version is an error
   (`AipdfError::InvalidXml`), and `extract_semantic_xml` therefore reports the
   layer as absent, leaving the PDF usable.

Migration contract for format authors:

- Bumping the **minor** version is for additive changes only; existing readers
  keep working.
- A **major** bump signals a breaking change. Producers SHOULD continue to
  embed a valid visual PDF layer so that older readers degrade to plain PDF, and
  MAY dual-embed a `1.x` payload alongside a new-major payload during a
  transition window (readers select the highest major they support).

The implemented major version is exposed programmatically as
`aipdf::SUPPORTED_MAJOR_VERSION`.

## Security

The semantic layer is declarative only.

Disallowed:

- JavaScript
- launch actions
- prompt templates
- model directives
- executable payloads
- external entity expansion
- network references
- embedded vector databases
- model-generated summaries or hidden instructions

Validators must reject XML containing processing instructions, DOCTYPE declarations, active content terms, or schema-invalid blocks.
