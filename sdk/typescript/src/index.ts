import { readFileSync } from "node:fs";
import { brotliDecompressSync } from "node:zlib";

const SEMANTIC_SUBTYPE = Buffer.from("/application#aipdf+xml+br");
const DISALLOWED_MARKERS = [
  "<!DOCTYPE",
  "<?xml-stylesheet",
  "<script",
  "/JavaScript",
  "/Launch",
  "system prompt",
  "model directive",
];

export interface SemanticBlock {
  kind: string;
  id?: string;
  page?: number;
  bbox?: string;
  text: string;
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

export function getReadingOrder(xml: string): SemanticBlock[] {
  const out: SemanticBlock[] = [];
  const tagPattern = /<(title|paragraph|caption|equation|citation|cell|item|codeBlock|reference|footnote|note)\b([^>]*)>([\s\S]*?)<\/\1>/g;
  for (const match of sanitizeXml(xml).matchAll(tagPattern)) {
    const attrs = parseAttrs(match[2]);
    out.push({
      kind: match[1],
      id: attrs.id,
      page: attrs.page ? Number(attrs.page) : undefined,
      bbox: attrs.bbox,
      text: stripTags(match[3]),
    });
  }
  return out;
}

export function collectElementText(xml: string, element: string): string[] {
  const escaped = element.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const pattern = new RegExp(`<${escaped}\\b[^>]*>([\\s\\S]*?)<\\/${escaped}>`, "g");
  return [...sanitizeXml(xml).matchAll(pattern)].map((m) => stripTags(m[1]));
}

export function xmlToMarkdown(xml: string): string {
  const clean = sanitizeXml(xml);
  const lines: string[] = [];
  let level = 1;
  // Extended token pattern handles: section, simple inline blocks, codeBlock,
  // list, references, figure, definitionList, table
  const tokenPattern =
    /<section\b([^>]*)>|<(title|paragraph|citation|equation|note|footnote)\b[^>]*>([\s\S]*?)<\/\2>|<codeBlock\b([^>]*)>([\s\S]*?)<\/codeBlock>|<(list)\b([^>]*)>([\s\S]*?)<\/list>|<(references)\b[^>]*>([\s\S]*?)<\/references>|<(figure)\b([^>]*)>([\s\S]*?)<\/figure>|<(definitionList)\b[^>]*>([\s\S]*?)<\/definitionList>|<table\b[^>]*>([\s\S]*?)<\/table>/g;
  for (const match of clean.matchAll(tokenPattern)) {
    if (match[0].startsWith("<section")) {
      // group 1: section attrs
      level = Number(parseAttrs(match[1]).level ?? "1");
    } else if (match[2]) {
      // group 2: simple inline tag name; group 3: content
      const tag = match[2];
      const text = stripTags(match[3]);
      if (tag === "title") {
        lines.push(`${"#".repeat(Math.min(Math.max(level, 1), 6))} ${text}`, "");
      } else if (tag === "paragraph") {
        lines.push(text, "");
      } else if (tag === "citation") {
        lines.push(`> ${text}`, "");
      } else if (tag === "equation") {
        lines.push("```math", text, "```", "");
      } else if (tag === "note") {
        lines.push(`> Note: ${text}`, "");
      } else if (tag === "footnote") {
        // Use a sequential footnote label derived from current count
        const idx = lines.filter((l) => l.startsWith("[^")).length + 1;
        lines.push(`[^note${idx}]: ${text}`, "");
      }
    } else if (match[4] !== undefined) {
      // group 4: codeBlock attrs; group 5: codeBlock content
      const lang = parseAttrs(match[4]).language ?? "";
      lines.push(`\`\`\`${lang}`, match[5].trim(), "```", "");
    } else if (match[6] === "list") {
      // group 7: list attrs; group 8: list body
      const listAttrs = parseAttrs(match[7]);
      const ordered = listAttrs.type === "ordered";
      const items = [...match[8].matchAll(/<item\b[^>]*>([\s\S]*?)<\/item>/g)];
      items.forEach((item, i) => {
        const text = stripTags(item[1]);
        lines.push(ordered ? `${i + 1}. ${text}` : `- ${text}`);
      });
      lines.push("");
    } else if (match[9] === "references") {
      // group 10: references body
      const refs = [...match[10].matchAll(/<reference\b[^>]*>([\s\S]*?)<\/reference>/g)];
      if (refs.length > 0) {
        refs.forEach((ref) => lines.push(`- ${stripTags(ref[1])}`));
        lines.push("");
      }
    } else if (match[11] === "figure") {
      // group 12: figure attrs; group 13: figure body
      const body = match[13];
      const imgAttrs = parseAttrs((body.match(/<image\b([^>]*)\/?\s*>/) ?? ["", ""])[1]);
      const captionText = stripTags((body.match(/<caption\b[^>]*>([\s\S]*?)<\/caption>/) ?? ["", ""])[1]);
      const alt = imgAttrs.alt ?? "";
      const src = imgAttrs.src ?? "";
      lines.push(`![${alt}](${src})`, "");
      if (captionText) lines.push(`_${captionText}_`, "");
    } else if (match[14] === "definitionList") {
      // group 15: definitionList body
      const defs = [...match[15].matchAll(/<definition\b([^>]*)>([\s\S]*?)<\/definition>/g)];
      defs.forEach((def) => {
        const term = parseAttrs(def[1]).term ?? "";
        const text = stripTags(def[2]);
        lines.push(`- ${term}: ${text}`);
      });
      lines.push("");
    } else {
      // table: last capture group — use the full match content between <table> tags
      const tableBody = match[0].replace(/^<table\b[^>]*>/, "").replace(/<\/table>$/, "");
      lines.push(...tableToMarkdown(tableBody), "");
    }
  }
  return lines.join("\n").trim();
}

function tableToMarkdown(xml: string): string[] {
  const rows = [...xml.matchAll(/<row\b[^>]*>([\s\S]*?)<\/row>/g)].map((row) =>
    [...row[1].matchAll(/<cell\b[^>]*>([\s\S]*?)<\/cell>/g)].map((cell) => stripTags(cell[1])),
  );
  if (rows.length === 0) return [];
  return [
    `| ${rows[0].join(" | ")} |`,
    `| ${rows[0].map(() => "---").join(" | ")} |`,
    ...rows.slice(1).map((row) => `| ${row.join(" | ")} |`),
  ];
}

function parseAttrs(attrs: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const match of attrs.matchAll(/([A-Za-z_:][A-Za-z0-9_.:-]*)=["']([^"']*)["']/g)) {
    out[match[1]] = match[2];
  }
  return out;
}

function stripTags(input: string): string {
  return input
    .replace(/<!\[CDATA\[([\s\S]*?)\]\]>/g, "$1")
    .replace(/<[^>]+>/g, " ")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&")
    .replace(/\s+/g, " ")
    .trim();
}

export function xmlToOnto(xml: string): string {
  const clean = sanitizeXml(xml);
  const version = parseAttrs((clean.match(/<document\b([^>]*)>/) ?? ["", ""])[1]).version ?? "";
  const metadata = (clean.match(/<metadata\b[^>]*>([\s\S]*?)<\/metadata>/) ?? ["", ""])[1];
  const title = stripTags((metadata.match(/<title\b[^>]*>([\s\S]*?)<\/title>/) ?? ["", ""])[1]);
  const sections: Record<string, string>[] = [];
  const blocks: Record<string, string>[] = [];
  const tables: Record<string, string>[] = [];
  const figures: Record<string, string>[] = [];
  const references: Record<string, string>[] = [];

  for (const sectionMatch of clean.matchAll(/<(section|appendix)\b([^>]*)>([\s\S]*?)<\/\1>/g)) {
    const tag = sectionMatch[1];
    const attrs = parseAttrs(sectionMatch[2]);
    const body = sectionMatch[3];
    const section = {
      id: attrs.id ?? "",
      level: attrs.level ?? (tag === "appendix" ? "appendix" : ""),
      page: attrs.page ?? attrs.pageStart ?? "",
      role: attrs.role ?? attrs.semanticRole ?? (tag === "appendix" ? "appendix" : ""),
      title: "",
    };
    sections.push(section);
    for (const blockMatch of body.matchAll(/<(title|paragraph|caption|equation|citation|item|note|footnote|definition|codeBlock|annotation)\b([^>]*)>([\s\S]*?)<\/\1>/g)) {
      const kind = blockMatch[1];
      if ((kind === "caption" && /<table\b[\s\S]*<caption\b/.test(body.slice(0, blockMatch.index ?? 0))) || kind === "caption") {
        // Captions are exported with their table/figure record when possible.
        const before = body.slice(0, blockMatch.index ?? 0);
        const after = body.slice(blockMatch.index ?? 0);
        if (before.lastIndexOf("<table") > before.lastIndexOf("</table>") || before.lastIndexOf("<figure") > before.lastIndexOf("</figure>")) continue;
        if (/^<caption\b[\s\S]*?<\/caption>\s*<\/(table|figure)>/.test(after)) continue;
      }
      const attrs = parseAttrs(blockMatch[2]);
      let text = stripTags(blockMatch[3]);
      if (kind === "definition" && attrs.term) text = `${attrs.term}: ${text}`;
      if (kind === "title" && !section.title) section.title = text;
      blocks.push({
        id: attrs.id ?? "",
        kind,
        section_id: section.id,
        level: section.level,
        page: attrs.page ?? section.page,
        bbox: attrs.bbox ?? "",
        role: attrs.role ?? defaultOntoRole(kind),
        text,
      });
    }
  }

  for (const tableMatch of clean.matchAll(/<table\b([^>]*)>([\s\S]*?)<\/table>/g)) {
    const attrs = parseAttrs(tableMatch[1]);
    const body = tableMatch[2];
    const rows = [...body.matchAll(/<row\b[^>]*>([\s\S]*?)<\/row>/g)]
      .map((row) => [...row[1].matchAll(/<cell\b[^>]*>([\s\S]*?)<\/cell>/g)].map((cell) => ontoArrayScalar(stripTags(cell[1]))).join("^"))
      .join("|");
    tables.push({
      id: attrs.id ?? "",
      page: attrs.page ?? "",
      bbox: attrs.bbox ?? "",
      caption: stripTags((body.match(/<caption\b[^>]*>([\s\S]*?)<\/caption>/) ?? ["", ""])[1]),
      rows,
    });
  }

  for (const figureMatch of clean.matchAll(/<figure\b([^>]*)>([\s\S]*?)<\/figure>/g)) {
    const attrs = parseAttrs(figureMatch[1]);
    const body = figureMatch[2];
    const imageAttrs = parseAttrs((body.match(/<image\b([^>]*)\/?\s*>/) ?? ["", ""])[1]);
    figures.push({
      id: attrs.id ?? "",
      page: attrs.page ?? "",
      bbox: attrs.bbox ?? "",
      caption: stripTags((body.match(/<caption\b[^>]*>([\s\S]*?)<\/caption>/) ?? ["", ""])[1]),
      alt: imageAttrs.alt ?? "",
      source: attrs.source ?? imageAttrs.src ?? "",
    });
  }

  for (const refMatch of clean.matchAll(/<reference\b([^>]*)>([\s\S]*?)<\/reference>/g)) {
    const attrs = parseAttrs(refMatch[1]);
    references.push({ id: attrs.id ?? "", type: attrs.type ?? "", text: stripTags(refMatch[2]) });
  }

  const lines = ["Document[1]:"];
  ontoField(lines, "version", version);
  ontoField(lines, "title", title);
  ontoField(lines, "source_format", "aipdf.semantic.xml");
  lines.push("");
  ontoColumns(lines, "Sections", sections, ["id", "level", "page", "role", "title"]);
  lines.push("");
  ontoColumns(lines, "Blocks", blocks, ["id", "kind", "section_id", "level", "page", "bbox", "role", "text"]);
  lines.push("");
  ontoColumns(lines, "Tables", tables, ["id", "page", "bbox", "caption", "rows"]);
  lines.push("");
  ontoColumns(lines, "Figures", figures, ["id", "page", "bbox", "caption", "alt", "source"]);
  lines.push("");
  ontoColumns(lines, "References", references, ["id", "type", "text"]);
  return lines.join("\n").trimEnd();
}

function ontoColumns(lines: string[], name: string, records: Record<string, string>[], fields: string[]): void {
  lines.push(`${name}[${records.length}]:`);
  for (const field of fields) lines.push(`    ${field}: ${records.map((record) => ontoScalar(record[field] ?? "")).join("|")}`);
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
