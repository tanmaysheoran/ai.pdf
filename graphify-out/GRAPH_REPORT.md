# Graph Report - aipdf  (2026-05-29)

## Corpus Check
- 91 files · ~177,197 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1048 nodes · 1959 edges · 60 communities (48 shown, 12 thin omitted)
- Extraction: 96% EXTRACTED · 4% INFERRED · 0% AMBIGUOUS · INFERRED: 81 edges (avg confidence: 0.77)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `f3c7fdc6`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- [[_COMMUNITY_Community 0|Community 0]]
- [[_COMMUNITY_Community 1|Community 1]]
- [[_COMMUNITY_Community 2|Community 2]]
- [[_COMMUNITY_Community 3|Community 3]]
- [[_COMMUNITY_Community 4|Community 4]]
- [[_COMMUNITY_Community 5|Community 5]]
- [[_COMMUNITY_Community 6|Community 6]]
- [[_COMMUNITY_Community 7|Community 7]]
- [[_COMMUNITY_Community 8|Community 8]]
- [[_COMMUNITY_Community 9|Community 9]]
- [[_COMMUNITY_Community 10|Community 10]]
- [[_COMMUNITY_Community 11|Community 11]]
- [[_COMMUNITY_Community 12|Community 12]]
- [[_COMMUNITY_Community 13|Community 13]]
- [[_COMMUNITY_Community 14|Community 14]]
- [[_COMMUNITY_Community 15|Community 15]]
- [[_COMMUNITY_Community 16|Community 16]]
- [[_COMMUNITY_Community 17|Community 17]]
- [[_COMMUNITY_Community 18|Community 18]]
- [[_COMMUNITY_Community 19|Community 19]]
- [[_COMMUNITY_Community 20|Community 20]]
- [[_COMMUNITY_Community 22|Community 22]]
- [[_COMMUNITY_Community 23|Community 23]]
- [[_COMMUNITY_Community 25|Community 25]]
- [[_COMMUNITY_Community 26|Community 26]]
- [[_COMMUNITY_Community 29|Community 29]]
- [[_COMMUNITY_Community 30|Community 30]]
- [[_COMMUNITY_Community 31|Community 31]]
- [[_COMMUNITY_Community 32|Community 32]]
- [[_COMMUNITY_Community 33|Community 33]]
- [[_COMMUNITY_Community 34|Community 34]]
- [[_COMMUNITY_Community 35|Community 35]]
- [[_COMMUNITY_Community 36|Community 36]]
- [[_COMMUNITY_Community 37|Community 37]]
- [[_COMMUNITY_Community 38|Community 38]]
- [[_COMMUNITY_Community 39|Community 39]]
- [[_COMMUNITY_Community 40|Community 40]]
- [[_COMMUNITY_Community 41|Community 41]]
- [[_COMMUNITY_Community 42|Community 42]]
- [[_COMMUNITY_Community 43|Community 43]]
- [[_COMMUNITY_Community 44|Community 44]]
- [[_COMMUNITY_Community 45|Community 45]]
- [[_COMMUNITY_Community 46|Community 46]]
- [[_COMMUNITY_Community 47|Community 47]]
- [[_COMMUNITY_Community 48|Community 48]]
- [[_COMMUNITY_Community 49|Community 49]]
- [[_COMMUNITY_Community 50|Community 50]]
- [[_COMMUNITY_Community 51|Community 51]]
- [[_COMMUNITY_Community 52|Community 52]]
- [[_COMMUNITY_Community 53|Community 53]]
- [[_COMMUNITY_Community 54|Community 54]]
- [[_COMMUNITY_Community 55|Community 55]]
- [[_COMMUNITY_Community 56|Community 56]]
- [[_COMMUNITY_Community 57|Community 57]]

## God Nodes (most connected - your core abstractions)
1. `Layout` - 25 edges
2. `build_rendered_pdf()` - 25 edges
3. `AIPDFError` - 20 edges
4. `typst_to_xml()` - 17 edges
5. `AIPDFDocument` - 15 edges
6. `sanitize_xml()` - 15 edges
7. `sanitizeXml()` - 15 edges
8. `ai.pdf` - 15 edges
9. `Layout` - 14 edges
10. `AIPDFDocument` - 14 edges

## Surprising Connections (you probably didn't know these)
- `build()` --calls--> `build_aipdf()`  [INFERRED]
  crates/aipdf/tests/roundtrip.rs → crates/aipdf/src/pdf/mod.rs
- `build()` --calls--> `build_aipdf()`  [INFERRED]
  crates/aipdf/tests/roundtrip.rs → crates/aipdf/src/pdf.rs
- `xml_to_markdown_ast()` --calls--> `heading_node()`  [INFERRED]
  crates/aipdf/src/markdown/ast.rs → crates/aipdf/src/markdown/ast_nodes.rs
- `xml_to_markdown_ast()` --calls--> `paragraph_node()`  [INFERRED]
  crates/aipdf/src/markdown/ast.rs → crates/aipdf/src/markdown/ast_nodes.rs
- `xml_to_markdown_ast()` --calls--> `image_paragraph_node()`  [INFERRED]
  crates/aipdf/src/markdown/ast.rs → crates/aipdf/src/markdown/ast_nodes.rs

## Communities (60 total, 12 thin omitted)

### Community 0 - "Community 0"
Cohesion: 0.05
Nodes (69): AIPDF, aipdfBinary(), AIPDFDocument, AIPDFError, astBlockquote(), astEmit(), astHeading(), astImageParagraph() (+61 more)

### Community 1 - "Community 1"
Cohesion: 0.12
Nodes (18): apply_coordinates(), Assembler, attr_val(), BlockCoord, build_rendered_pdf(), DocElem, elem_id(), EncodedImage (+10 more)

### Community 2 - "Community 2"
Cohesion: 0.14
Nodes (24): collect_cells_from_row(), collect_list_items(), collect_rows_from(), collect_table(), element_text(), extract_code_language(), flush_typst_para(), heading_level() (+16 more)

### Community 3 - "Community 3"
Cohesion: 0.06
Nodes (41): build_aipdf(), BuildOptions, InspectReport, RenderMode, brotli_compress(), build_aipdf(), BuildOptions, collect_image_xobjects() (+33 more)

### Community 4 - "Community 4"
Cohesion: 0.06
Nodes (68): _ast_blockquote(), _ast_emit(), _ast_heading(), _ast_image_paragraph(), _ast_list_item(), _ast_paragraph(), _ast_table(), _ast_text() (+60 more)

### Community 5 - "Community 5"
Cohesion: 0.14
Nodes (24): attr_value(), BlockRecord, Capture, CaptureKind, column(), column_raw(), default_role(), encode_array_scalar() (+16 more)

### Community 6 - "Community 6"
Cohesion: 0.15
Nodes (9): default_font_has_unicode_glyphs(), encode_records_glyphs_and_skips_notdef(), flate(), Font, font_file2(), font_file2_compresses_and_tounicode_maps(), GlyphSet, tounicode_cmap() (+1 more)

### Community 7 - "Community 7"
Cohesion: 0.21
Nodes (18): aipdf_binary(), bench(), build(), export(), ExportResult, extract_images(), ingest(), _parse_saved() (+10 more)

### Community 8 - "Community 8"
Cohesion: 0.22
Nodes (13): build_from_source(), extract_semantic_xml(), html_to_xml(), _make_xml_document(), markdown_to_xml(), open(), Convert source text to semantic XML.      kind must be one of: 'xml', 'markdown', Serialise a list of section dicts into a <document> XML string.      Each sectio (+5 more)

### Community 9 - "Community 9"
Cohesion: 0.10
Nodes (25): extract_xml_payload(), semantic_xml_from_source(), SourceKind, build_aipdf_browser(), chrome_available(), file_url(), find_chrome(), inject_print_styles() (+17 more)

### Community 10 - "Community 10"
Cohesion: 0.11
Nodes (23): extract_semantic_xml(), largest_page_jpeg(), ocr_page(), split_paragraphs(), tesseract_available(), attach_semantic_layer(), ingest_pdf(), IngestOptions (+15 more)

### Community 11 - "Community 11"
Cohesion: 0.21
Nodes (16): attr_value(), blockquote_node(), heading_node(), image_paragraph_node(), list_item_node(), MarkdownAst, MarkdownNode, paragraph_node() (+8 more)

### Community 12 - "Community 12"
Cohesion: 0.17
Nodes (14): call_tool(), _error(), handle(), _open(), A minimal Model Context Protocol (MCP) server for ``.ai.pdf`` files.  It lets an, Handle one JSON-RPC message; return a response, or None for notifications., Run the stdio JSON-RPC loop until EOF., Dispatch a tool call and return its text result. (+6 more)

### Community 13 - "Community 13"
Cohesion: 0.18
Nodes (10): AIPDFDocument, collect_element_text(), get_reading_order(), Validate the embedded semantic XML (matches `aipdf validate`)., Serialize the document's MDAST tree as pretty JSON (Rust-conformant)., Validate the embedded semantic XML (matches `aipdf validate`)., sanitize_xml(), SemanticBlock (+2 more)

### Community 14 - "Community 14"
Cohesion: 0.14
Nodes (13): description, devDependencies, @types/node, typescript, license, main, name, scripts (+5 more)

### Community 16 - "Community 16"
Cohesion: 0.23
Nodes (10): compressed, outPath, stream(), visibleContentStream(), visibleText, writePdf(), xml, xmlEscape() (+2 more)

### Community 17 - "Community 17"
Cohesion: 0.27
Nodes (8): accepts_any_1_x(), check_supported_version(), collect_element_text(), doc(), find_citations(), get_tables(), SemanticBlock, validate_xml()

### Community 18 - "Community 18"
Cohesion: 0.20
Nodes (9): compilerOptions, declaration, module, moduleResolution, outDir, skipLibCheck, strict, target (+1 more)

### Community 19 - "Community 19"
Cohesion: 0.29
Nodes (6): data, doc, maximal, rep, sample, xml

### Community 20 - "Community 20"
Cohesion: 0.67
Nodes (6): case(), markdown_ast_matches_golden(), markdown_matches_golden(), onto_matches_golden(), read(), root()

### Community 22 - "Community 22"
Cohesion: 0.40
Nodes (5): cases, golden(), root, trimEnd(), xml

### Community 29 - "Community 29"
Cohesion: 0.06
Nodes (69): astBlockquote(), astEmit(), astHeading(), astImageParagraph(), astListItem(), astParagraph(), astTable(), astText() (+61 more)

### Community 30 - "Community 30"
Cohesion: 0.07
Nodes (26): Architecture, CLI options, code:bash (cargo test                    # run all Rust tests), code:bash (python3 -m venv .venv), code:bash (cd sdk/typescript), code:bash (.venv/bin/python tests/mcp_smoke.py          # drive the std), code:bash (cargo test -p aipdf --test conformance        # Rust vs gold), code:block6 (Input (XML/MD/HTML/Typst)) (+18 more)

### Community 31 - "Community 31"
Cohesion: 0.07
Nodes (27): ai.pdf, CLI, code:xml (<document version="1.0">), code:block10 (crates/), code:bash (git clone https://github.com/tanmaysheoran/ai.pdf.git), code:bash (# Build from XML, HTML, or Markdown), code:bash (aipdf-mcp          # or: python -m aipdf.mcp_server), code:toml ([dependencies]) (+19 more)

### Community 32 - "Community 32"
Cohesion: 0.12
Nodes (13): heading_level(), HtmlConverter, ListCtx, TableCtx, markdown_to_xml(), wrap_document(), flush_typst_para(), is_typst_list_item() (+5 more)

### Community 33 - "Community 33"
Cohesion: 0.13
Nodes (8): flate(), font_file2(), tounicode_cmap(), default_font_has_unicode_glyphs(), encode_records_glyphs_and_skips_notdef(), Font, font_file2_compresses_and_tounicode_maps(), GlyphSet

### Community 34 - "Community 34"
Cohesion: 0.13
Nodes (17): brotli_compress(), decompress_semantic(), find_bytes(), find_semantic_compressed(), find_semantic_stream(), find_semantic_stream_lopdf(), decode_xobject(), extract_images() (+9 more)

### Community 35 - "Community 35"
Cohesion: 0.13
Nodes (3): Layout, PageOptions, wrap_words()

### Community 36 - "Community 36"
Cohesion: 0.10
Nodes (20): aipdf V1 File Format Specification, code:text (aipdf-semantic.xml.br), code:pdf (/AIPDFNote (AIPDF semantic layer present: extract aipdf-sema), code:text (application/aipdf+xml+br), code:pdf (/Subtype /application#2Faipdf+xml+br), code:text (1 0 obj  Catalog), code:pdf (<<), code:xml (<document version="1.0" id="doc1" lang="en">) (+12 more)

### Community 37 - "Community 37"
Cohesion: 0.15
Nodes (10): Assembler, pdf_str(), stream_obj(), apply_coordinates(), build_rendered_pdf(), elem_id(), attr_val(), BlockCoord (+2 more)

### Community 38 - "Community 38"
Cohesion: 0.19
Nodes (15): attr_value(), finish_capture(), is_inside(), new_capture(), tag_name(), xml_to_onto(), default_role(), encode_array_scalar() (+7 more)

### Community 39 - "Community 39"
Cohesion: 0.14
Nodes (4): EncodedImage, ImageObj, load_image(), Layout

### Community 40 - "Community 40"
Cohesion: 0.27
Nodes (11): blockquote_node(), heading_node(), image_paragraph_node(), list_item_node(), paragraph_node(), table_node(), text_node(), value_node() (+3 more)

### Community 41 - "Community 41"
Cohesion: 0.18
Nodes (8): BlockRecord, Capture, CaptureKind, FigureRecord, ReferenceRecord, SectionContext, SectionRecord, TableRecord

### Community 43 - "Community 43"
Cohesion: 0.56
Nodes (8): collect_cells_from_row(), collect_list_items(), collect_rows_from(), collect_table(), element_text(), extract_code_language(), html_to_xml(), walk_elements()

### Community 44 - "Community 44"
Cohesion: 0.25
Nodes (7): find_semantic_stream(), inspect_pdf(), InspectReport, Byte-level report matching `aipdf inspect` (re-reads the file)., Report PDF / semantic-layer presence and byte sizes (matches Rust `inspect_pdf`), Mirror of the Rust core's `InspectReport` (pdf.rs) / `aipdf inspect`., Byte-level report matching `aipdf inspect` (re-reads the file).

### Community 46 - "Community 46"
Cohesion: 0.25
Nodes (7): aipdf MCP server, Client configuration, code:bash (python3 -m pip install -e sdk/python   # provides the `aipdf), code:json ({), Install and run, What it exposes, Why ONTO by default

### Community 47 - "Community 47"
Cohesion: 0.33
Nodes (5): code:math (E = m c^2), code:rust (fn main() { println!("hi"); }), Data, Overview, Rich Conformance Sample

### Community 48 - "Community 48"
Cohesion: 0.33
Nodes (5): Architecture Diagrams, Build Pipeline, code:mermaid (flowchart LR), code:mermaid (flowchart LR), Parse Pipeline

### Community 49 - "Community 49"
Cohesion: 0.33
Nodes (5): Controls, Residual Risk, Security Analysis, Semantic Integrity Checks, Threat Model

### Community 50 - "Community 50"
Cohesion: 0.40
Nodes (4): Benchmark Comparisons, Compression Analysis, Method, Prototype Sample

### Community 52 - "Community 52"
Cohesion: 0.50
Nodes (3): Automated Prototype Tests, Manual Test Matrix, PDF Compatibility Tests

## Knowledge Gaps
- **187 isolated node(s):** `SemanticBlock`, `AipdfError`, `MarkdownAst`, `MarkdownNode`, `OcrMode` (+182 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **12 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `main()` connect `Community 9` to `Community 10`, `Community 3`?**
  _High betweenness centrality (0.033) - this node is a cross-community bridge._
- **Why does `build_aipdf()` connect `Community 3` to `Community 9`, `Community 1`?**
  _High betweenness centrality (0.028) - this node is a cross-community bridge._
- **Why does `build_rendered_pdf()` connect `Community 1` to `Community 3`?**
  _High betweenness centrality (0.022) - this node is a cross-community bridge._
- **Are the 3 inferred relationships involving `AIPDFDocument` (e.g. with `AIPDFError` and `SemanticBlock`) actually correct?**
  _`AIPDFDocument` has 3 INFERRED edges - model-reasoned connections that need verification._
- **What connects `SemanticBlock`, `AipdfError`, `MarkdownAst` to the rest of the system?**
  _226 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Community 0` be split into smaller, more focused modules?**
  _Cohesion score 0.05126050420168067 - nodes in this community are weakly interconnected._
- **Should `Community 1` be split into smaller, more focused modules?**
  _Cohesion score 0.12066365007541478 - nodes in this community are weakly interconnected._