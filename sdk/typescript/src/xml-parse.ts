import { SemanticBlock } from "./types.js";
import { sanitizeXml } from "./sanitize.js";

export interface XmlNode {
  tag: string; // element tag, or "#text" for text nodes
  attrs: Record<string, string>;
  children: XmlNode[];
  value?: string; // text content for "#text" nodes
}

export function decodeEntities(text: string): string {
  return text
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'")
    .replace(/&#x([0-9a-fA-F]+);/g, (_, h) => String.fromCodePoint(parseInt(h, 16)))
    .replace(/&#(\d+);/g, (_, d) => String.fromCodePoint(parseInt(d, 10)))
    .replace(/&amp;/g, "&");
}

export function parseAttrs(attrs: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const match of attrs.matchAll(/([A-Za-z_:][A-Za-z0-9_.:-]*)=["']([^"']*)["']/g)) {
    out[match[1]] = decodeEntities(match[2]);
  }
  return out;
}

/** Parse XML into a DOM and return the root document element. */
export function parseXml(xml: string): XmlNode {
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
export function textOf(node: XmlNode): string {
  let acc = "";
  const visit = (nd: XmlNode) => {
    if (nd.tag === "#text") acc += nd.value ?? "";
    else nd.children.forEach(visit);
  };
  visit(node);
  return acc.replace(/\s+/g, " ").trim();
}

export function elementChildren(node: XmlNode): XmlNode[] {
  return node.children.filter((c) => c.tag !== "#text");
}

export function findChild(node: XmlNode, tag: string): XmlNode | undefined {
  return node.children.find((c) => c.tag === tag);
}

/** Pre-order iteration over every element node (self included). */
export function* iterElements(node: XmlNode): Generator<XmlNode> {
  if (node.tag !== "#text" && node.tag !== "#root") yield node;
  for (const c of node.children) if (c.tag !== "#text") yield* iterElements(c);
}

export const READING_ORDER_KINDS = new Set([
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
