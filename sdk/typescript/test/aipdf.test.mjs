import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import { AIPDF, AIPDFDocument, extractSemanticXml, findSemanticStream, inspectPdf, xmlToMarkdown, xmlToOnto } from "../dist/index.js";

const sample = new URL("../../../samples/minimal.pdf", import.meta.url);
const maximal = new URL("../../../samples/maximal.pdf", import.meta.url);

test("extracts Brotli semantic XML from a valid PDF", () => {
  const data = readFileSync(sample);
  assert.equal(data.subarray(0, 5).toString(), "%PDF-");
  assert.ok(findSemanticStream(data));
  const xml = extractSemanticXml(data);
  assert.match(xml, /<document version="1.0"/);
  assert.match(xml, /<table/);
});

test("opens sample through SDK and exports markdown", () => {
  const doc = AIPDF.open(sample.pathname);
  assert.equal(doc.hasSemanticLayer, true);
  assert.equal(doc.getTables().length, 1);
  assert.equal(doc.findCitations().length, 1);
  assert.match(doc.toMarkdown(), /# Introduction/);
  assert.match(doc.toMarkdown(), /\| Target \| Limit \|/);
});

test("ordinary PDF fallback does not throw", () => {
  const doc = new AIPDFDocument(undefined, undefined, true);
  assert.equal(doc.hasSemanticLayer, false);
});

test("inspect reports byte counts and validates", () => {
  const rep = inspectPdf(readFileSync(sample));
  assert.equal(rep.isPdf, true);
  assert.equal(rep.hasSemanticLayer, true);
  assert.ok(rep.semanticCompressedBytes > 0);
  assert.ok(rep.semanticXmlBytes > 0);
  const doc = AIPDF.open(sample.pathname);
  assert.deepEqual(doc.inspect(), rep);
  assert.equal(doc.validate(), true);
});

test("markdown conversion is deterministic", () => {
  const xml = extractSemanticXml(readFileSync(sample));
  assert.equal(xmlToMarkdown(xml), xmlToMarkdown(xml));
  assert.equal(xmlToOnto(xml), xmlToOnto(xml));
});

test("opens extended maximal sample", () => {
  const doc = AIPDF.open(maximal.pathname);
  assert.equal(doc.hasSemanticLayer, true);
  assert.match(doc.toMarkdown(), /Mathematical Compression Model/);
  assert.match(doc.toOnto(), /Mathematical Compression Model/);
  assert.match(doc.toOnto(), /Figures\[1\]/);
  assert.equal(doc.getTables().length, 1);
});
