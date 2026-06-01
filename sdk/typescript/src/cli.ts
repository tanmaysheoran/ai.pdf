// ---------------------------------------------------------------------------
// CLI-delegating write-side + image helpers.
// The pure-JS read path needs no binary. But `build` (PDF assembly, font
// embedding, Brotli *compression*), `ingest` (lopdf + OCR), and image
// extraction (XObject decode + raster re-encode) are owned by the Rust core,
// so these shell out to the installed `aipdf` binary — the same pattern the
// MCP server uses. Set AIPDF_BIN to override the binary path (default: aipdf).
//
// The `postinstall` script (scripts/postinstall.mjs) downloads a pre-built
// binary into <package-root>/bin/ at npm-install time. aipdfBinary() checks
// that location first so the SDK works without a separate system install.
// ---------------------------------------------------------------------------

import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { AIPDFError, BuildOptions, IngestOptions, ExportFormat, ExportResult } from "./types.js";

// Absolute path to the binary downloaded by postinstall, if it exists.
const _localBin = (() => {
  try {
    const pkgRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
    const ext = process.platform === "win32" ? ".exe" : "";
    return join(pkgRoot, "bin", `aipdf${ext}`);
  } catch {
    return null;
  }
})();

export function aipdfBinary(): string {
  if (process.env.AIPDF_BIN) return process.env.AIPDF_BIN;
  if (_localBin && existsSync(_localBin)) return _localBin;
  return "aipdf";
}

export function runCli(args: string[]): string {
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
