import { test } from "node:test";
import assert from "node:assert";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { xmlToOnto, xmlToMarkdown } from "../dist/index.js";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const cases = {
  minimal: join(root, "samples", "minimal.xml"),
  rich: join(root, "tests", "conformance", "rich.xml"),
};

// The TypeScript SDK must reproduce the Rust-authored golden fixtures exactly,
// so all three implementations stay pinned to one source of truth.
for (const [name, xmlPath] of Object.entries(cases)) {
  const xml = readFileSync(xmlPath, "utf8");
  test(`${name} ONTO matches golden`, () => {
    const want = readFileSync(join(root, "tests", "conformance", `${name}.onto`), "utf8").replace(/\s+$/, "");
    assert.strictEqual(xmlToOnto(xml).replace(/\s+$/, ""), want);
  });
  test(`${name} markdown matches golden`, () => {
    const want = readFileSync(join(root, "tests", "conformance", `${name}.md`), "utf8").replace(/\s+$/, "");
    assert.strictEqual(xmlToMarkdown(xml).replace(/\s+$/, ""), want);
  });
}
