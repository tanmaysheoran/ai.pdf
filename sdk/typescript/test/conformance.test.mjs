import { test } from "node:test";
import assert from "node:assert";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { xmlToOnto, xmlToMarkdown, xmlToMarkdownAstJson } from "../dist/index.js";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const cases = {
  minimal: join(root, "samples", "minimal.xml"),
  rich: join(root, "tests", "conformance", "rich.xml"),
};

const trimEnd = (s) => s.replace(/\s+$/, "");
const golden = (name, ext) => trimEnd(readFileSync(join(root, "tests", "conformance", `${name}.${ext}`), "utf8"));

// The TypeScript SDK must reproduce the Rust-authored golden fixtures exactly,
// so all three implementations stay pinned to one source of truth.
for (const [name, xmlPath] of Object.entries(cases)) {
  const xml = readFileSync(xmlPath, "utf8");
  test(`${name} ONTO matches golden`, () => {
    assert.strictEqual(trimEnd(xmlToOnto(xml)), golden(name, "onto"));
  });
  test(`${name} markdown matches golden`, () => {
    assert.strictEqual(trimEnd(xmlToMarkdown(xml)), golden(name, "md"));
  });
}

// `markdown-ast` has a golden only for `rich` (it carries a <figure>/<image>),
// which guards the regression where self-closing <image/> nodes were dropped.
test("rich markdown-ast matches golden", () => {
  const xml = readFileSync(cases.rich, "utf8");
  assert.strictEqual(trimEnd(xmlToMarkdownAstJson(xml)), golden("rich", "ast.json"));
});
