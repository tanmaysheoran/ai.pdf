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

export type RenderMode = "minimal" | "full" | "browser";
export type PageSize = "letter" | "a4";
export type OcrMode = "auto" | "never" | "force";
export type ExportFormat = "xml" | "markdown" | "markdown-ast" | "onto";

export interface ExportResult {
  output: string;
  images: string[];
}

export interface BuildOptions {
  output?: string;
  render?: RenderMode;
  pageSize?: PageSize;
  font?: string;
  title?: string;
}

export interface IngestOptions {
  output?: string;
  ocr?: OcrMode;
  lang?: string;
}
