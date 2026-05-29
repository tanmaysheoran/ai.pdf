// ---------------------------------------------------------------------------
// buildFromSource — convert source text to semantic XML
// ---------------------------------------------------------------------------

import { sanitizeXml } from "./sanitize.js";
import { stripTags } from "./ast.js";
import { htmlToXml } from "./source-html.js";

export function buildFromSource(source: string, kind: "xml" | "markdown" | "html" | "typst"): string {
  switch (kind) {
    case "xml": return xmlSourceToXml(source);
    case "markdown": return markdownToXml(source);
    case "html": return htmlToXml(source);
    case "typst": return typstToXml(source);
  }
}

export function xmlDocHeader(): string {
  return '<?xml version="1.0" encoding="UTF-8"?>\n<document version="1.0" id="doc1" lang="en">\n';
}

export function xmlDocFooter(): string {
  return "\n</document>\n";
}

/** Strip ```xml fences and return sanitized XML. */
export function xmlSourceToXml(source: string): string {
  const stripped = source.replace(/^```xml\s*/i, "").replace(/\s*```\s*$/, "").trim();
  return sanitizeXml(stripped);
}

// ---- Markdown → XML -------------------------------------------------------

export function escapeXml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

export function markdownToXml(source: string): string {
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

// ---- Typst → XML ----------------------------------------------------------

export function typstToXml(source: string): string {
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
