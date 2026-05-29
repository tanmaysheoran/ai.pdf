// --- Markdown AST (MDAST) export --------------------------------------------
// Mirrors the Rust core's streaming walker (`xml_to_markdown_ast` in
// markdown.rs) so the JSON is byte-for-byte identical. `mdNode` emits keys in
// the Rust struct's field order (type, value, depth, lang, ordered, url, alt,
// children) and omits absent ones, matching serde's `skip_serializing_if`;
// `JSON.stringify(_, null, 2)` then matches serde_json's pretty output (raw
// UTF-8, 2-space indent).

import { sanitizeXml } from "./sanitize.js";
import { XmlNode, parseXml, elementChildren, findChild, iterElements } from "./xml-parse.js";

export interface MdastNode {
  type: string;
  value?: string;
  depth?: number;
  lang?: string;
  ordered?: boolean;
  url?: string;
  alt?: string;
  children?: MdastNode[];
}

export function mdNode(fields: MdastNode): MdastNode {
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
export function astText(node: XmlNode): string {
  let acc = "";
  const visit = (nd: XmlNode) => {
    if (nd.tag === "#text") acc += (nd.value ?? "").trim();
    else nd.children.forEach(visit);
  };
  visit(node);
  return acc;
}

export const astTextNode = (value: string): MdastNode => mdNode({ type: "text", value });
export const astHeading = (depth: number, value: string): MdastNode =>
  mdNode({ type: "heading", depth: Math.min(Math.max(depth, 1), 6), children: [astTextNode(value)] });
export const astParagraph = (value: string): MdastNode => mdNode({ type: "paragraph", children: [astTextNode(value)] });
export const astImageParagraph = (src: string, alt: string): MdastNode =>
  mdNode({ type: "paragraph", children: [mdNode({ type: "image", url: src, alt })] });
export const astBlockquote = (value: string): MdastNode => mdNode({ type: "blockquote", children: [astParagraph(value)] });
export const astListItem = (value: string): MdastNode => mdNode({ type: "listItem", children: [astParagraph(value)] });
export const astValue = (type: string, value: string, lang?: string): MdastNode => mdNode({ type, value, lang });
export const astTable = (rows: string[][]): MdastNode =>
  mdNode({
    type: "table",
    children: rows.map((row) =>
      mdNode({ type: "tableRow", children: row.map((cell) => mdNode({ type: "tableCell", children: [astTextNode(cell)] })) })),
  });

export function astEmit(elem: XmlNode, out: MdastNode[], state: { level: number }): void {
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

export function stripTags(input: string): string {
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
