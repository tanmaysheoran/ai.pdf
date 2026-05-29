import { sanitizeXml } from "./sanitize.js";
import { XmlNode, parseXml, elementChildren, findChild, textOf } from "./xml-parse.js";

export const ONTO_BLOCK_KINDS = new Set([
  "title", "paragraph", "caption", "equation", "citation", "item",
  "note", "footnote", "definition", "codeBlock", "annotation",
]);

export interface OntoSection {
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

export function ontoColumns(
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

export function ontoField(lines: string[], name: string, value: string): void {
  lines.push(`    ${name}: ${ontoScalar(value)}`);
}

export function ontoScalar(value: string): string {
  return value.replace(/\s+/g, " ").trim().replace(/`/g, "'").replace(/\|/g, "/").replace(/\^/g, ";");
}

export function ontoArrayScalar(value: string): string {
  return ontoScalar(value);
}

export function defaultOntoRole(tag: string): string {
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
