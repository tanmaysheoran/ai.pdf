import { readFileSync } from "node:fs";
import { brotliDecompressSync } from "node:zlib";
import { execFileSync } from "node:child_process";

// Conformant PDF name for MIME application/aipdf+xml+br ('/' escaped as #2F).
const SEMANTIC_SUBTYPE = Buffer.from("/application#2Faipdf+xml+br");
// Active-content / structural markers only. Kept in lockstep with the Rust core
// (security.rs) and Python SDK. Natural-language phrases are intentionally NOT
// banned: XML text is data, never instructions, and the visible PDF already
// carries the same words.
const DISALLOWED_MARKERS = [
  "<!DOCTYPE",
  "<?xml-stylesheet",
  "<?processing",
  "<script",
  "/JavaScript",
  "/Launch",
];

export interface SemanticBlock {
  kind: string;
  id?: string;
  page?: number;
  bbox?: string;
  text: string;
}

/** Mirror of the Rust core's `InspectReport` (pdf.rs) / `aipdf inspect`. */
export interface InspectReport {
  isPdf: boolean;
  hasSemanticLayer: boolean;
  semanticCompressedBytes?: number;
  semanticXmlBytes?: number;
}

export class AIPDFError extends Error {}

export class AIPDF {
  static open(path: string): AIPDFDocument {
    return AIPDFDocument.open(path);
  }
}

export class AIPDFDocument {
  constructor(
    readonly path: string | undefined,
    readonly xml: string | undefined,
    readonly isPdf: boolean,
  ) {}

  static open(path: string): AIPDFDocument {
    const data = readFileSync(path);
    const xml = extractSemanticXml(data);
    return new AIPDFDocument(path, xml, data.subarray(0, 5).toString() === "%PDF-");
  }

  get hasSemanticLayer(): boolean {
    return this.xml !== undefined;
  }

  /** Byte-level report matching `aipdf inspect` (re-reads the backing file). */
  inspect(): InspectReport {
    if (this.path === undefined) throw new AIPDFError("document has no backing file path");
    return inspectPdf(readFileSync(this.path));
  }

  /** Validate the embedded semantic XML (matches `aipdf validate`). */
  validate(): boolean {
    validateXml(this.toXml());
    return true;
  }

  toXml(): string {
    if (!this.xml) throw new AIPDFError("semantic layer not found");
    return this.xml;
  }

  getStructure(): SemanticBlock[] {
    return getReadingOrder(this.toXml());
  }

  toMarkdown(): string {
    return xmlToMarkdown(this.toXml());
  }

  toMarkdownAst(): string {
    return xmlToMarkdownAstJson(this.toXml());
  }

  toOnto(): string {
    return xmlToOnto(this.toXml());
  }

  getTables(): string[] {
    return collectElementText(this.toXml(), "table");
  }

  getReadingOrder(): SemanticBlock[] {
    return getReadingOrder(this.toXml());
  }

  findCitations(): string[] {
    return collectElementText(this.toXml(), "citation");
  }
}

export function extractSemanticXml(data: Buffer): string | undefined {
  const stream = findSemanticStream(data);
  if (!stream) return undefined;
  const xml = brotliDecompressSync(stream).toString("utf8");
  validateXml(xml);
  return xml;
}

/** Report PDF / semantic-layer presence and byte sizes (matches Rust `inspect_pdf`). */
export function inspectPdf(data: Buffer): InspectReport {
  const isPdf = data.subarray(0, 5).toString() === "%PDF-";
  const stream = findSemanticStream(data);
  if (!stream) return { isPdf, hasSemanticLayer: false };
  try {
    // Mirror Rust's decompress_semantic: sanitize (trim) then measure UTF-8 bytes.
    const xml = sanitizeXml(brotliDecompressSync(stream).toString("utf8"));
    return {
      isPdf,
      hasSemanticLayer: true,
      semanticCompressedBytes: stream.length,
      semanticXmlBytes: Buffer.byteLength(xml, "utf8"),
    };
  } catch {
    return { isPdf, hasSemanticLayer: false, semanticCompressedBytes: stream.length };
  }
}

export function findSemanticStream(data: Buffer): Buffer | undefined {
  const markerPos = data.indexOf(SEMANTIC_SUBTYPE);
  if (markerPos < 0) return undefined;
  const streamPos = data.indexOf(Buffer.from("stream\n"), markerPos);
  if (streamPos < 0) return undefined;
  const start = streamPos + "stream\n".length;
  const end = data.indexOf(Buffer.from("\nendstream"), start);
  if (end < 0) return undefined;
  return data.subarray(start, end);
}

export function validateXml(xml: string): void {
  const sanitized = sanitizeXml(xml);
  if (!/^<\?xml[\s\S]*?<document\b|^<document\b/.test(sanitized)) {
    throw new AIPDFError("root element must be <document>");
  }
  if (!/<document\b[^>]*\bversion=["'][^"']+["']/.test(sanitized)) {
    throw new AIPDFError("document version must be present");
  }
  if (!/<section\b[^>]*\bid=["'][^"']+["']/.test(sanitized)) {
    throw new AIPDFError("document must contain at least one section with an id");
  }
  for (const match of sanitized.matchAll(/\bbox=["']([^"']+)["']/g)) {
    if (!/^-?\d+(\.\d+)?,-?\d+(\.\d+)?,-?\d+(\.\d+)?,-?\d+(\.\d+)?$/.test(match[1])) {
      throw new AIPDFError(`invalid bbox: ${match[1]}`);
    }
  }
}

export function sanitizeXml(xml: string): string {
  const clean = xml.replace(/^\uFEFF/, "").trim();
  const lowered = clean.toLowerCase();
  for (const marker of DISALLOWED_MARKERS) {
    if (lowered.includes(marker.toLowerCase())) {
      throw new AIPDFError(`disallowed marker \`${marker}\``);
    }
  }
  if (Buffer.byteLength(clean, "utf8") > 16 * 1024 * 1024) {
    throw new AIPDFError("semantic XML exceeds 16 MiB safety limit");
  }
  return clean;
}

// ---------------------------------------------------------------------------
// Minimal but proper XML parser (replaces the previous regex-based consumer).
// Produces a small DOM that the read-side transforms walk in document order —
// robust to nesting, attribute quoting, CDATA, comments, and entities.
// ---------------------------------------------------------------------------

interface XmlNode {
  tag: string; // element tag, or "#text" for text nodes
  attrs: Record<string, string>;
  children: XmlNode[];
  value?: string; // text content for "#text" nodes
}

function decodeEntities(text: string): string {
  return text
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'")
    .replace(/&#x([0-9a-fA-F]+);/g, (_, h) => String.fromCodePoint(parseInt(h, 16)))
    .replace(/&#(\d+);/g, (_, d) => String.fromCodePoint(parseInt(d, 10)))
    .replace(/&amp;/g, "&");
}

function parseAttrs(attrs: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const match of attrs.matchAll(/([A-Za-z_:][A-Za-z0-9_.:-]*)=["']([^"']*)["']/g)) {
    out[match[1]] = decodeEntities(match[2]);
  }
  return out;
}

/** Parse XML into a DOM and return the root document element. */
function parseXml(xml: string): XmlNode {
  const root: XmlNode = { tag: "#root", attrs: {}, children: [] };
  const stack: XmlNode[] = [root];
  const top = () => stack[stack.length - 1];
  let i = 0;
  const n = xml.length;
  while (i < n) {
    if (xml[i] === "<") {
      if (xml.startsWith("<!--", i)) {
        const e = xml.indexOf("-->", i);
        i = e < 0 ? n : e + 3;
      } else if (xml.startsWith("<![CDATA[", i)) {
        const e = xml.indexOf("]]>", i + 9);
        top().children.push({ tag: "#text", attrs: {}, children: [], value: xml.slice(i + 9, e < 0 ? n : e) });
        i = e < 0 ? n : e + 3;
      } else if (xml.startsWith("<?", i)) {
        const e = xml.indexOf("?>", i);
        i = e < 0 ? n : e + 2;
      } else if (xml.startsWith("<!", i)) {
        const e = xml.indexOf(">", i);
        i = e < 0 ? n : e + 1;
      } else if (xml[i + 1] === "/") {
        const e = xml.indexOf(">", i);
        if (stack.length > 1) stack.pop();
        i = e < 0 ? n : e + 1;
      } else {
        const e = xml.indexOf(">", i);
        if (e < 0) break;
        let inner = xml.slice(i + 1, e).trim();
        const selfClose = inner.endsWith("/");
        if (selfClose) inner = inner.slice(0, -1).trim();
        const sp = inner.search(/\s/);
        const tag = sp < 0 ? inner : inner.slice(0, sp);
        const attrs = sp < 0 ? {} : parseAttrs(inner.slice(sp));
        const node: XmlNode = { tag, attrs, children: [] };
        top().children.push(node);
        if (!selfClose) stack.push(node);
        i = e + 1;
      }
    } else {
      const e = xml.indexOf("<", i);
      const raw = xml.slice(i, e < 0 ? n : e);
      if (raw.length) top().children.push({ tag: "#text", attrs: {}, children: [], value: decodeEntities(raw) });
      i = e < 0 ? n : e;
    }
  }
  return root.children.find((c) => c.tag !== "#text") ?? root;
}

/** All descendant text, whitespace-normalised (matches Rust/Python text_of). */
function textOf(node: XmlNode): string {
  let acc = "";
  const visit = (nd: XmlNode) => {
    if (nd.tag === "#text") acc += nd.value ?? "";
    else nd.children.forEach(visit);
  };
  visit(node);
  return acc.replace(/\s+/g, " ").trim();
}

function elementChildren(node: XmlNode): XmlNode[] {
  return node.children.filter((c) => c.tag !== "#text");
}

function findChild(node: XmlNode, tag: string): XmlNode | undefined {
  return node.children.find((c) => c.tag === tag);
}

/** Pre-order iteration over every element node (self included). */
function* iterElements(node: XmlNode): Generator<XmlNode> {
  if (node.tag !== "#text" && node.tag !== "#root") yield node;
  for (const c of node.children) if (c.tag !== "#text") yield* iterElements(c);
}

const READING_ORDER_KINDS = new Set([
  "title", "paragraph", "caption", "equation", "citation", "cell",
  "item", "codeBlock", "reference", "footnote", "note",
]);

export function getReadingOrder(xml: string): SemanticBlock[] {
  const root = parseXml(sanitizeXml(xml));
  const out: SemanticBlock[] = [];
  for (const el of iterElements(root)) {
    if (READING_ORDER_KINDS.has(el.tag)) {
      out.push({
        kind: el.tag,
        id: el.attrs.id,
        page: el.attrs.page ? Number(el.attrs.page) : undefined,
        bbox: el.attrs.bbox,
        text: textOf(el),
      });
    }
  }
  return out;
}

export function collectElementText(xml: string, element: string): string[] {
  const root = parseXml(sanitizeXml(xml));
  const out: string[] = [];
  for (const el of iterElements(root)) if (el.tag === element) out.push(textOf(el));
  return out;
}

export function xmlToMarkdown(xml: string): string {
  const root = parseXml(sanitizeXml(xml));
  const lines: string[] = [];
  renderChildren(root, lines, 1);
  return lines.join("\n").trim();
}

function renderChildren(elem: XmlNode, lines: string[], level: number): void {
  for (const child of elementChildren(elem)) {
    switch (child.tag) {
      case "section":
      case "appendix": {
        const childLevel = Number(child.attrs.level ?? level) || level;
        renderChildren(child, lines, childLevel);
        break;
      }
      case "title":
        lines.push(`${"#".repeat(Math.min(Math.max(level, 1), 6))} ${textOf(child)}`, "");
        break;
      case "paragraph":
        lines.push(textOf(child), "");
        break;
      case "citation":
        lines.push(`> ${textOf(child)}`, "");
        break;
      case "equation":
        lines.push("```math", textOf(child), "```", "");
        break;
      case "table":
        renderTable(child, lines);
        break;
      case "list": {
        const ordered = child.attrs.type === "ordered";
        elementChildren(child)
          .filter((c) => c.tag === "item")
          .forEach((item, i) => lines.push(ordered ? `${i + 1}. ${textOf(item)}` : `- ${textOf(item)}`));
        lines.push("");
        break;
      }
      case "codeBlock": {
        const lang = child.attrs.language ?? "";
        lines.push(`\`\`\`${lang}`, textOf(child), "```", "");
        break;
      }
      case "note":
        lines.push(`> Note: ${textOf(child)}`, "");
        break;
      case "footnote":
        lines.push(`[^note]: ${textOf(child)}`, "");
        break;
      case "references":
        for (const ref of elementChildren(child)) if (ref.tag === "reference") lines.push(`- ${textOf(ref)}`);
        lines.push("");
        break;
      case "figure": {
        const image = findChild(child, "image");
        const alt = child.attrs.alt || image?.attrs.alt || "";
        const src = image?.attrs.src ?? "";
        const cap = findChild(child, "caption");
        lines.push(`![${alt}](${src})`, "");
        if (cap) lines.push(`_${textOf(cap)}_`, "");
        break;
      }
      case "definitionList":
        for (const defn of elementChildren(child))
          if (defn.tag === "definition") lines.push(`- ${defn.attrs.term ?? ""}: ${textOf(defn)}`);
        lines.push("");
        break;
      default:
        renderChildren(child, lines, level);
    }
  }
}

function renderTable(table: XmlNode, lines: string[]): void {
  const cap = findChild(table, "caption");
  if (cap) {
    const text = textOf(cap);
    if (text) lines.push(`_${text}_`, "");
  }
  const rows: string[][] = [];
  const collectRows = (parent: XmlNode) => {
    for (const row of elementChildren(parent)) {
      if (row.tag === "row") rows.push(elementChildren(row).filter((c) => c.tag === "cell").map(textOf));
    }
  };
  const thead = findChild(table, "thead");
  if (thead) collectRows(thead);
  const tbody = findChild(table, "tbody");
  if (tbody) collectRows(tbody);
  collectRows(table);
  if (rows.length === 0) return;
  lines.push(`| ${rows[0].join(" | ")} |`);
  lines.push(`| ${rows[0].map(() => "---").join(" | ")} |`);
  for (const row of rows.slice(1)) lines.push(`| ${row.join(" | ")} |`);
  lines.push("");
}

// --- Markdown AST (MDAST) export --------------------------------------------
// Mirrors the Rust core's streaming walker (`xml_to_markdown_ast` in
// markdown.rs) so the JSON is byte-for-byte identical. `mdNode` emits keys in
// the Rust struct's field order (type, value, depth, lang, ordered, url, alt,
// children) and omits absent ones, matching serde's `skip_serializing_if`;
// `JSON.stringify(_, null, 2)` then matches serde_json's pretty output (raw
// UTF-8, 2-space indent).

interface MdastNode {
  type: string;
  value?: string;
  depth?: number;
  lang?: string;
  ordered?: boolean;
  url?: string;
  alt?: string;
  children?: MdastNode[];
}

function mdNode(fields: MdastNode): MdastNode {
  const node: MdastNode = { type: fields.type };
  if (fields.value !== undefined) node.value = fields.value;
  if (fields.depth !== undefined) node.depth = fields.depth;
  if (fields.lang !== undefined) node.lang = fields.lang;
  if (fields.ordered !== undefined) node.ordered = fields.ordered;
  if (fields.url !== undefined) node.url = fields.url;
  if (fields.alt !== undefined) node.alt = fields.alt;
  if (fields.children !== undefined && fields.children.length > 0) node.children = fields.children;
  return node;
}

// Rust concatenates each trimmed text run (no internal whitespace
// normalization), unlike `textOf`. For single-run elements they coincide.
function astText(node: XmlNode): string {
  let acc = "";
  const visit = (nd: XmlNode) => {
    if (nd.tag === "#text") acc += (nd.value ?? "").trim();
    else nd.children.forEach(visit);
  };
  visit(node);
  return acc;
}

const astTextNode = (value: string): MdastNode => mdNode({ type: "text", value });
const astHeading = (depth: number, value: string): MdastNode =>
  mdNode({ type: "heading", depth: Math.min(Math.max(depth, 1), 6), children: [astTextNode(value)] });
const astParagraph = (value: string): MdastNode => mdNode({ type: "paragraph", children: [astTextNode(value)] });
const astImageParagraph = (src: string, alt: string): MdastNode =>
  mdNode({ type: "paragraph", children: [mdNode({ type: "image", url: src, alt })] });
const astBlockquote = (value: string): MdastNode => mdNode({ type: "blockquote", children: [astParagraph(value)] });
const astListItem = (value: string): MdastNode => mdNode({ type: "listItem", children: [astParagraph(value)] });
const astValue = (type: string, value: string, lang?: string): MdastNode => mdNode({ type, value, lang });
const astTable = (rows: string[][]): MdastNode =>
  mdNode({
    type: "table",
    children: rows.map((row) =>
      mdNode({ type: "tableRow", children: row.map((cell) => mdNode({ type: "tableCell", children: [astTextNode(cell)] })) })),
  });

function astEmit(elem: XmlNode, out: MdastNode[], state: { level: number }): void {
  for (const child of elementChildren(elem)) {
    switch (child.tag) {
      case "section": {
        const lvl = parseInt(child.attrs.level ?? "", 10);
        state.level = Number.isNaN(lvl) ? 1 : lvl;
        astEmit(child, out, state);
        break;
      }
      case "title":
        out.push(astHeading(state.level, astText(child)));
        break;
      case "paragraph":
      case "caption":
        out.push(astParagraph(astText(child)));
        break;
      case "citation":
        out.push(astBlockquote(astText(child)));
        break;
      case "equation":
        out.push(astValue("math", astText(child)));
        break;
      case "codeBlock":
        out.push(astValue("code", astText(child), child.attrs.language));
        break;
      case "note":
        out.push(astBlockquote(`Note: ${astText(child)}`));
        break;
      case "footnote":
        out.push(mdNode({ type: "footnoteDefinition", children: [astParagraph(astText(child))] }));
        break;
      case "image":
        out.push(astImageParagraph(child.attrs.src ?? "", child.attrs.alt ?? ""));
        break;
      case "list":
      case "references":
      case "definitionList": {
        const items: MdastNode[] = [];
        for (const sub of elementChildren(child)) {
          if (sub.tag === "item" || sub.tag === "reference") items.push(astListItem(astText(sub)));
          else if (sub.tag === "definition") {
            const term = sub.attrs.term ?? "";
            items.push(astListItem(term ? `${term}: ${astText(sub)}` : astText(sub)));
          }
        }
        out.push(mdNode({ type: "list", ordered: false, children: items }));
        break;
      }
      case "table": {
        const cap = findChild(child, "caption");
        if (cap) out.push(astParagraph(astText(cap)));
        const rows: string[][] = [];
        for (const row of iterElements(child))
          if (row.tag === "row") rows.push(elementChildren(row).filter((c) => c.tag === "cell").map(astText));
        out.push(astTable(rows));
        break;
      }
      default:
        astEmit(child, out, state);
    }
  }
}

export function xmlToMarkdownAst(xml: string): MdastNode {
  const root = parseXml(sanitizeXml(xml));
  const children: MdastNode[] = [];
  astEmit(root, children, { level: 1 });
  return { type: "root", children };
}

export function xmlToMarkdownAstJson(xml: string): string {
  return JSON.stringify(xmlToMarkdownAst(xml), null, 2);
}

function stripTags(input: string): string {
  // Retained for the source-side HTML converter below.
  return input
    .replace(/<!\[CDATA\[([\s\S]*?)\]\]>/g, "$1")
    .replace(/<[^>]+>/g, " ")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&")
    .replace(/\s+/g, " ")
    .trim();
}

const ONTO_BLOCK_KINDS = new Set([
  "title", "paragraph", "caption", "equation", "citation", "item",
  "note", "footnote", "definition", "codeBlock", "annotation",
]);

interface OntoSection {
  id: string;
  level: string;
  page: string;
  role: string;
  title: string;
}

export function xmlToOnto(xml: string): string {
  const root = parseXml(sanitizeXml(xml));
  const version = root.attrs.version ?? "";
  const metaTitleEl = findChild(root, "metadata") && findChild(findChild(root, "metadata")!, "title");
  const docTitle = metaTitleEl ? textOf(metaTitleEl) : "";

  const sections: OntoSection[] = [];
  const blocks: Record<string, string>[] = [];
  const tables: { id: string; page: string; bbox: string; caption: string; rows: string[][] }[] = [];
  const figures: Record<string, string>[] = [];
  const references: Record<string, string>[] = [];

  const walk = (node: XmlNode, section: OntoSection | null, inMetadata: boolean) => {
    const tag = node.tag;
    let current = section;
    if (tag === "section" || tag === "appendix") {
      current = {
        id: node.attrs.id ?? "",
        level: node.attrs.level ?? (tag === "appendix" ? "appendix" : ""),
        page: node.attrs.page ?? node.attrs.pageStart ?? "",
        role: node.attrs.role ?? node.attrs.semanticRole ?? (tag === "appendix" ? "appendix" : ""),
        title: "",
      };
      sections.push(current);
    } else if (tag === "metadata") {
      inMetadata = true;
    } else if (tag === "table") {
      tables.push({
        id: node.attrs.id ?? "",
        page: node.attrs.page ?? "",
        bbox: node.attrs.bbox ?? "",
        caption: (() => { const c = findChild(node, "caption"); return c ? textOf(c) : ""; })(),
        rows: elementChildren(node)
          .filter((r) => r.tag === "row")
          .map((r) => elementChildren(r).filter((c) => c.tag === "cell").map(textOf)),
      });
      return; // captions/cells belong to the table record, not the block list
    } else if (tag === "figure") {
      const image = findChild(node, "image");
      const cap = findChild(node, "caption");
      figures.push({
        id: node.attrs.id ?? "",
        page: node.attrs.page ?? "",
        bbox: node.attrs.bbox ?? "",
        caption: cap ? textOf(cap) : "",
        alt: image?.attrs.alt ?? "",
        source: node.attrs.source ?? image?.attrs.src ?? "",
      });
      return;
    } else if (tag === "reference") {
      references.push({ id: node.attrs.id ?? "", type: node.attrs.type ?? "", text: textOf(node) });
      return;
    } else if (ONTO_BLOCK_KINDS.has(tag)) {
      if (current && !inMetadata) {
        let text = textOf(node);
        if (tag === "definition" && node.attrs.term) text = `${node.attrs.term}: ${text}`;
        if (tag === "title" && !current.title) current.title = text;
        blocks.push({
          id: node.attrs.id ?? "",
          kind: tag,
          section_id: current.id,
          level: current.level,
          page: node.attrs.page ?? current.page,
          bbox: node.attrs.bbox ?? "",
          role: node.attrs.role ?? defaultOntoRole(tag),
          text,
        });
      }
    }
    for (const child of elementChildren(node)) walk(child, current, inMetadata);
  };
  walk(root, null, false);

  const lines = ["Document[1]:"];
  ontoField(lines, "version", version);
  ontoField(lines, "title", docTitle);
  ontoField(lines, "source_format", "aipdf.semantic.xml");
  lines.push("");
  ontoColumns(lines, "Sections", sections as unknown as Record<string, string>[], ["id", "level", "page", "role", "title"]);
  lines.push("");
  ontoColumns(lines, "Blocks", blocks, ["id", "kind", "section_id", "level", "page", "bbox", "role", "text"]);
  lines.push("");
  const tableRecords = tables.map((t) => ({
    id: t.id,
    page: t.page,
    bbox: t.bbox,
    caption: t.caption,
    // Pre-serialised with raw ^ / | delimiters; emitted via rawFields below.
    rows: t.rows.map((row) => row.map(ontoArrayScalar).join("^")).join("|"),
  }));
  ontoColumns(lines, "Tables", tableRecords, ["id", "page", "bbox", "caption", "rows"], new Set(["rows"]));
  lines.push("");
  ontoColumns(lines, "Figures", figures, ["id", "page", "bbox", "caption", "alt", "source"]);
  lines.push("");
  ontoColumns(lines, "References", references, ["id", "type", "text"]);
  return lines.join("\n").trimEnd();
}

function ontoColumns(
  lines: string[],
  name: string,
  records: Record<string, string>[],
  fields: string[],
  rawFields: Set<string> = new Set(),
): void {
  lines.push(`${name}[${records.length}]:`);
  for (const field of fields) {
    const cells = records.map((record) =>
      rawFields.has(field) ? String(record[field] ?? "") : ontoScalar(record[field] ?? ""),
    );
    lines.push(`    ${field}: ${cells.join("|")}`);
  }
}

function ontoField(lines: string[], name: string, value: string): void {
  lines.push(`    ${name}: ${ontoScalar(value)}`);
}

function ontoScalar(value: string): string {
  return value.replace(/\s+/g, " ").trim().replace(/`/g, "'").replace(/\|/g, "/").replace(/\^/g, ";");
}

function ontoArrayScalar(value: string): string {
  return ontoScalar(value);
}

function defaultOntoRole(tag: string): string {
  return ({
    title: "heading",
    paragraph: "body",
    caption: "caption",
    equation: "equation",
    citation: "citation",
    item: "list-item",
    note: "note",
    footnote: "footnote",
    definition: "definition",
    codeBlock: "code",
    annotation: "annotation",
  } as Record<string, string>)[tag] ?? tag;
}

// ---------------------------------------------------------------------------
// buildFromSource — convert source text to semantic XML
// ---------------------------------------------------------------------------

export function buildFromSource(source: string, kind: "xml" | "markdown" | "html" | "typst"): string {
  switch (kind) {
    case "xml": return xmlSourceToXml(source);
    case "markdown": return markdownToXml(source);
    case "html": return htmlToXml(source);
    case "typst": return typstToXml(source);
  }
}

function xmlDocHeader(): string {
  return '<?xml version="1.0" encoding="UTF-8"?>\n<document version="1.0" id="doc1" lang="en">\n';
}

function xmlDocFooter(): string {
  return "\n</document>\n";
}

/** Strip ```xml fences and return sanitized XML. */
function xmlSourceToXml(source: string): string {
  const stripped = source.replace(/^```xml\s*/i, "").replace(/\s*```\s*$/, "").trim();
  return sanitizeXml(stripped);
}

// ---- Markdown → XML -------------------------------------------------------

function escapeXml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function markdownToXml(source: string): string {
  const lines = source.split(/\r?\n/);
  let sectionCount = 0;
  let blockCount = 0;
  const xmlParts: string[] = [xmlDocHeader()];

  // Collect lines into sections delimited by headings
  type Section = { level: number; title: string; lines: string[] };
  const sections: Section[] = [];
  let currentSection: Section | null = null;

  for (const line of lines) {
    const headingMatch = line.match(/^(#{1,6})\s+(.*)/);
    if (headingMatch) {
      if (currentSection) sections.push(currentSection);
      currentSection = { level: headingMatch[1].length, title: headingMatch[2].trim(), lines: [] };
    } else if (currentSection) {
      currentSection.lines.push(line);
    } else {
      // Content before first heading goes into an implicit section
      currentSection = { level: 1, title: "", lines: [line] };
    }
  }
  if (currentSection) sections.push(currentSection);

  // If no sections were found, wrap everything in one section
  if (sections.length === 0) {
    sections.push({ level: 1, title: "", lines: lines });
  }

  for (const sec of sections) {
    sectionCount++;
    blockCount++;
    const secId = `s${sectionCount}`;
    const titleId = `b${blockCount}`;
    xmlParts.push(`  <section id="${secId}" level="${sec.level}" page="1">`);
    if (sec.title) {
      xmlParts.push(`    <title id="${titleId}" page="1" role="title">${escapeXml(sec.title)}</title>`);
    }

    // Parse body lines into blocks
    let i = 0;
    const bodyLines = sec.lines;

    // Helper: consume a fenced code block
    while (i < bodyLines.length) {
      const line = bodyLines[i];

      // Blank line
      if (line.trim() === "") { i++; continue; }

      // Fenced code block
      const fenceMatch = line.match(/^```(\w*)/);
      if (fenceMatch) {
        const lang = fenceMatch[1];
        const codeLines: string[] = [];
        i++;
        while (i < bodyLines.length && !bodyLines[i].startsWith("```")) {
          codeLines.push(bodyLines[i]);
          i++;
        }
        i++; // consume closing ```
        blockCount++;
        const langAttr = lang ? ` language="${escapeXml(lang)}"` : "";
        xmlParts.push(`    <codeBlock id="b${blockCount}" page="1"${langAttr}>${escapeXml(codeLines.join("\n"))}</codeBlock>`);
        continue;
      }

      // Unordered list
      if (/^[-*+]\s/.test(line)) {
        blockCount++;
        xmlParts.push(`    <list id="b${blockCount}" page="1" type="unordered">`);
        while (i < bodyLines.length && /^[-*+]\s/.test(bodyLines[i])) {
          blockCount++;
          const itemText = bodyLines[i].replace(/^[-*+]\s+/, "");
          xmlParts.push(`      <item id="b${blockCount}" page="1">${escapeXml(itemText)}</item>`);
          i++;
        }
        xmlParts.push("    </list>");
        continue;
      }

      // Ordered list
      if (/^\d+\.\s/.test(line)) {
        blockCount++;
        xmlParts.push(`    <list id="b${blockCount}" page="1" type="ordered">`);
        while (i < bodyLines.length && /^\d+\.\s/.test(bodyLines[i])) {
          blockCount++;
          const itemText = bodyLines[i].replace(/^\d+\.\s+/, "");
          xmlParts.push(`      <item id="b${blockCount}" page="1">${escapeXml(itemText)}</item>`);
          i++;
        }
        xmlParts.push("    </list>");
        continue;
      }

      // Blockquote → note
      if (/^>\s/.test(line)) {
        const noteLines: string[] = [];
        while (i < bodyLines.length && /^>\s?/.test(bodyLines[i])) {
          noteLines.push(bodyLines[i].replace(/^>\s?/, ""));
          i++;
        }
        blockCount++;
        xmlParts.push(`    <note id="b${blockCount}" page="1">${escapeXml(noteLines.join(" ").trim())}</note>`);
        continue;
      }

      // Footnote reference [^label]: text
      const footnoteMatch = line.match(/^\[\^([^\]]+)\]:\s*(.*)/);
      if (footnoteMatch) {
        blockCount++;
        xmlParts.push(`    <footnote id="b${blockCount}" page="1">${escapeXml(footnoteMatch[2])}</footnote>`);
        i++;
        continue;
      }

      // Regular paragraph — accumulate until blank line
      const paraLines: string[] = [];
      while (i < bodyLines.length && bodyLines[i].trim() !== "" && !/^(#{1,6}\s|```|[-*+]\s|\d+\.\s|>\s)/.test(bodyLines[i])) {
        paraLines.push(bodyLines[i]);
        i++;
      }
      if (paraLines.length > 0) {
        blockCount++;
        xmlParts.push(`    <paragraph id="b${blockCount}" page="1">${escapeXml(paraLines.join(" ").trim())}</paragraph>`);
      }
    }

    xmlParts.push("  </section>");
  }

  xmlParts.push(xmlDocFooter());
  return xmlParts.join("\n");
}

// ---- HTML → XML -----------------------------------------------------------

function htmlToXml(source: string): string {
  let sectionCount = 0;
  let blockCount = 0;
  const xmlParts: string[] = [xmlDocHeader()];

  // Strip HTML comments
  const clean = source.replace(/<!--[\s\S]*?-->/g, "");

  // Extract body content if present
  const bodyMatch = clean.match(/<body\b[^>]*>([\s\S]*?)<\/body>/i);
  const body = bodyMatch ? bodyMatch[1] : clean;

  // Tokenise top-level elements
  const tokenRe =
    /<(h[1-6]|p|ul|ol|blockquote|pre|figure|table)\b([^>]*)>([\s\S]*?)<\/\1>|<hr\b[^>]*\/?>/gi;

  // We'll group content into sections when h-tags appear
  type HtmlSection = { level: number; title: string; tokens: Array<{ tag: string; attrs: string; body: string }> };
  const sections: HtmlSection[] = [];
  let currentSection: HtmlSection | null = null;

  for (const m of body.matchAll(tokenRe)) {
    const tag = m[1]?.toLowerCase() ?? "hr";
    const attrs = m[2] ?? "";
    const content = m[3] ?? "";

    if (/^h[1-6]$/.test(tag)) {
      if (currentSection) sections.push(currentSection);
      const lvl = parseInt(tag[1], 10);
      currentSection = { level: lvl, title: stripTags(content), tokens: [] };
    } else {
      if (!currentSection) currentSection = { level: 1, title: "", tokens: [] };
      currentSection.tokens.push({ tag, attrs, body: content });
    }
  }
  if (currentSection) sections.push(currentSection);
  if (sections.length === 0) sections.push({ level: 1, title: "", tokens: [] });

  for (const sec of sections) {
    sectionCount++;
    const secId = `s${sectionCount}`;
    xmlParts.push(`  <section id="${secId}" level="${sec.level}" page="1">`);
    if (sec.title) {
      blockCount++;
      xmlParts.push(`    <title id="b${blockCount}" page="1" role="title">${escapeXml(sec.title)}</title>`);
    }

    for (const tok of sec.tokens) {
      const { tag, body: tokBody } = tok;

      if (tag === "p") {
        blockCount++;
        xmlParts.push(`    <paragraph id="b${blockCount}" page="1">${escapeXml(stripTags(tokBody))}</paragraph>`);
      } else if (tag === "ul" || tag === "ol") {
        const listType = tag === "ol" ? "ordered" : "unordered";
        const items = [...tokBody.matchAll(/<li\b[^>]*>([\s\S]*?)<\/li>/gi)];
        if (items.length > 0) {
          blockCount++;
          xmlParts.push(`    <list id="b${blockCount}" page="1" type="${listType}">`);
          for (const item of items) {
            blockCount++;
            xmlParts.push(`      <item id="b${blockCount}" page="1">${escapeXml(stripTags(item[1]))}</item>`);
          }
          xmlParts.push("    </list>");
        }
      } else if (tag === "blockquote") {
        blockCount++;
        xmlParts.push(`    <note id="b${blockCount}" page="1">${escapeXml(stripTags(tokBody))}</note>`);
      } else if (tag === "pre") {
        const codeMatch = tokBody.match(/<code\b([^>]*)>([\s\S]*?)<\/code>/i);
        const langAttr = codeMatch ? (codeMatch[1].match(/class=["']language-(\w+)["']/) ?? [])[1] ?? "" : "";
        const codeText = codeMatch ? stripTags(codeMatch[2]) : stripTags(tokBody);
        blockCount++;
        const langXml = langAttr ? ` language="${escapeXml(langAttr)}"` : "";
        xmlParts.push(`    <codeBlock id="b${blockCount}" page="1"${langXml}>${escapeXml(codeText)}</codeBlock>`);
      } else if (tag === "figure") {
        blockCount++;
        const figId = `b${blockCount}`;
        const imgMatch = tokBody.match(/<img\b([^>]*)\/?\s*>/i);
        const imgAttrs = imgMatch ? parseAttrs(imgMatch[1]) : {};
        const captionMatch = tokBody.match(/<figcaption\b[^>]*>([\s\S]*?)<\/figcaption>/i);
        const caption = captionMatch ? stripTags(captionMatch[1]) : "";
        xmlParts.push(`    <figure id="${figId}" page="1">`);
        if (imgMatch) {
          xmlParts.push(`      <image src="${escapeXml(imgAttrs.src ?? "")}" alt="${escapeXml(imgAttrs.alt ?? "")}"/>`);
        }
        if (caption) {
          blockCount++;
          xmlParts.push(`      <caption id="b${blockCount}" page="1">${escapeXml(caption)}</caption>`);
        }
        xmlParts.push("    </figure>");
      } else if (tag === "table") {
        blockCount++;
        const tableId = `b${blockCount}`;
        xmlParts.push(`    <table id="${tableId}" page="1">`);
        const rows = [...tokBody.matchAll(/<tr\b[^>]*>([\s\S]*?)<\/tr>/gi)];
        for (const row of rows) {
          xmlParts.push("      <row>");
          const cells = [...row[1].matchAll(/<(th|td)\b[^>]*>([\s\S]*?)<\/\1>/gi)];
          for (const cell of cells) {
            blockCount++;
            xmlParts.push(`        <cell id="b${blockCount}" page="1">${escapeXml(stripTags(cell[2]))}</cell>`);
          }
          xmlParts.push("      </row>");
        }
        xmlParts.push("    </table>");
      }
    }

    xmlParts.push("  </section>");
  }

  xmlParts.push(xmlDocFooter());
  return xmlParts.join("\n");
}

// ---------------------------------------------------------------------------
// CLI-delegating write-side + image helpers.
// The pure-JS read path needs no binary. But `build` (PDF assembly, font
// embedding, Brotli *compression*), `ingest` (lopdf + OCR), and image
// extraction (XObject decode + raster re-encode) are owned by the Rust core,
// so these shell out to the installed `aipdf` binary — the same pattern the
// MCP server uses. Set AIPDF_BIN to override the binary path (default: aipdf).
// ---------------------------------------------------------------------------

export type RenderMode = "minimal" | "full" | "browser";
export type PageSize = "letter" | "a4";
export type OcrMode = "auto" | "never" | "force";
export type ExportFormat = "xml" | "markdown" | "markdown-ast" | "onto";

export interface ExportResult {
  output: string;
  images: string[];
}

export function aipdfBinary(): string {
  return process.env.AIPDF_BIN ?? "aipdf";
}

function runCli(args: string[]): string {
  try {
    return execFileSync(aipdfBinary(), args, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
  } catch (err) {
    const e = err as NodeJS.ErrnoException & { stderr?: Buffer | string; status?: number };
    if (e.code === "ENOENT") {
      throw new AIPDFError(
        `aipdf CLI not found (tried '${aipdfBinary()}'); build it with \`cargo build -p aipdf-cli\` or set AIPDF_BIN to its path`,
      );
    }
    const stderr = e.stderr ? e.stderr.toString().trim() : "";
    throw new AIPDFError(stderr || `aipdf failed (exit ${e.status ?? "?"})`);
  }
}

export interface BuildOptions {
  output?: string;
  render?: RenderMode;
  pageSize?: PageSize;
  font?: string;
  title?: string;
}

/** Build a `.ai.pdf` from a source file (.xml/.md/.html/.typ). Returns the written path. */
export function buildPdf(input: string, opts: BuildOptions = {}): string {
  const args = [
    "build", input,
    "--render", opts.render ?? "minimal",
    "--page-size", opts.pageSize ?? "letter",
  ];
  if (opts.title !== undefined) args.push("--title", opts.title);
  if (opts.font !== undefined) args.push("--font", opts.font);
  if (opts.output !== undefined) args.push("-o", opts.output);
  return runCli(args).trim();
}

export interface IngestOptions {
  output?: string;
  ocr?: OcrMode;
  lang?: string;
}

/** Attach a semantic layer to an existing PDF (text extraction + optional OCR). */
export function ingest(input: string, opts: IngestOptions = {}): string {
  const args = ["ingest", input, "--ocr", opts.ocr ?? "auto", "--lang", opts.lang ?? "eng"];
  if (opts.output !== undefined) args.push("-o", opts.output);
  return runCli(args).trim();
}

/** Export the semantic layer to a directory, returning the content + image paths. */
export function exportSave(file: string, save: string, format: ExportFormat = "markdown"): ExportResult {
  const out = runCli(["export", file, "--format", format, "--save", save]);
  const saved = out
    .split("\n")
    .filter((l) => l.startsWith("saved:"))
    .map((l) => l.slice("saved:".length).trim());
  if (saved.length === 0) throw new AIPDFError("export --save produced no output files");
  // First `saved:` line is the content file; the rest are extracted images.
  return { output: saved[0], images: saved.slice(1) };
}

/** Extract embedded raster images to `outDir`, returning their paths. */
export function extractImages(file: string, outDir: string, format: ExportFormat = "markdown"): string[] {
  return exportSave(file, outDir, format).images;
}

/** Run `aipdf bench` and return its reported `key: value` pairs. */
export function bench(input: string): Record<string, string> {
  const out = runCli(["bench", input]);
  const report: Record<string, string> = {};
  for (const line of out.split("\n")) {
    const idx = line.indexOf(":");
    if (idx > 0) report[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
  }
  return report;
}

// ---- Typst → XML ----------------------------------------------------------

function typstToXml(source: string): string {
  const lines = source.split(/\r?\n/);
  let sectionCount = 0;
  let blockCount = 0;
  const xmlParts: string[] = [xmlDocHeader()];

  type TypstSection = { level: number; title: string; paras: string[] };
  const sections: TypstSection[] = [];
  let currentSection: TypstSection | null = null;
  let pendingLines: string[] = [];

  const flushPending = () => {
    if (pendingLines.length > 0 && currentSection) {
      const text = pendingLines.join(" ").trim();
      if (text) currentSection.paras.push(text);
      pendingLines = [];
    }
  };

  for (const line of lines) {
    // Typst headings: = Title (level 1), == Title (level 2), etc.
    const headingMatch = line.match(/^(=+)\s+(.*)/);
    if (headingMatch) {
      flushPending();
      if (currentSection) sections.push(currentSection);
      currentSection = { level: headingMatch[1].length, title: headingMatch[2].trim(), paras: [] };
    } else if (line.trim() === "") {
      flushPending();
    } else {
      if (!currentSection) currentSection = { level: 1, title: "", paras: [] };
      pendingLines.push(line.trim());
    }
  }
  flushPending();
  if (currentSection) sections.push(currentSection);
  if (sections.length === 0) sections.push({ level: 1, title: "", paras: [] });

  for (const sec of sections) {
    sectionCount++;
    const secId = `s${sectionCount}`;
    xmlParts.push(`  <section id="${secId}" level="${sec.level}" page="1">`);
    if (sec.title) {
      blockCount++;
      xmlParts.push(`    <title id="b${blockCount}" page="1" role="title">${escapeXml(sec.title)}</title>`);
    }
    for (const para of sec.paras) {
      blockCount++;
      xmlParts.push(`    <paragraph id="b${blockCount}" page="1">${escapeXml(para)}</paragraph>`);
    }
    xmlParts.push("  </section>");
  }

  xmlParts.push(xmlDocFooter());
  return xmlParts.join("\n");
}
