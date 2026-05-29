// ---------------------------------------------------------------------------
// htmlToXml — convert an HTML document to semantic XML
// ---------------------------------------------------------------------------

import { parseAttrs } from "./xml-parse.js";
import { stripTags } from "./ast.js";
import { xmlDocHeader, xmlDocFooter, escapeXml } from "./source.js";

export function htmlToXml(source: string): string {
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
