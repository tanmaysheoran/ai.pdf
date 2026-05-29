import { sanitizeXml } from "./sanitize.js";
import { XmlNode, parseXml, elementChildren, findChild, textOf } from "./xml-parse.js";

export function xmlToMarkdown(xml: string): string {
  const root = parseXml(sanitizeXml(xml));
  const lines: string[] = [];
  renderChildren(root, lines, 1);
  return lines.join("\n").trim();
}

export function renderChildren(elem: XmlNode, lines: string[], level: number): void {
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

export function renderTable(table: XmlNode, lines: string[]): void {
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
