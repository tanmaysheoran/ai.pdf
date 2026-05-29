import { readFileSync } from "node:fs";
import { brotliDecompressSync } from "node:zlib";
import { SEMANTIC_SUBTYPE } from "./constants.js";
import { SemanticBlock, InspectReport, AIPDFError } from "./types.js";
import { sanitizeXml, validateXml } from "./sanitize.js";
import { getReadingOrder, collectElementText } from "./xml-parse.js";
import { xmlToMarkdown } from "./markdown.js";
import { xmlToMarkdownAstJson } from "./ast.js";
import { xmlToOnto } from "./onto.js";

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
