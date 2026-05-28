import { readFileSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { brotliCompressSync, constants } from "node:zlib";

const xmlPath = new URL("../samples/minimal.xml", import.meta.url);
const outPath = new URL("../samples/minimal.aipdf", import.meta.url);
const xml = readFileSync(xmlPath, "utf8").trim();
const compressed = brotliCompressSync(Buffer.from(xml), {
  params: {
    [constants.BROTLI_PARAM_QUALITY]: 6,
    [constants.BROTLI_PARAM_LGWIN]: 22,
  },
});

const visibleText = [
  "Introduction",
  "AI-native documents combine visual fidelity with semantic structure.",
  "Compression targets",
].join("\n");

writeFileSync(outPath, writePdf("Minimal AIPDF Sample", visibleText, xml, compressed));

function writePdf(title, visibleText, xml, compressed) {
  const content = visibleContentStream(visibleText);
  const xmp = xmpMetadata(title, Buffer.byteLength(xml), compressed.length);
  const checksum = createHash("sha256").update(compressed).digest("hex");
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R /Metadata 6 0 R /Names << /EmbeddedFiles 7 0 R >> /AF [8 0 R] >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    stream(Buffer.from(content), "<< >>"),
    stream(Buffer.from(xmp), "<< /Type /Metadata /Subtype /XML >>"),
    "<< /Names [(aipdf-semantic.xml.br) 8 0 R] >>",
    "<< /Type /Filespec /F (aipdf-semantic.xml.br) /UF (aipdf-semantic.xml.br) /Desc (Minimal AIPDF Sample semantic XML) /AFRelationship /Data /EF << /F 9 0 R /UF 9 0 R >> >>",
    stream(
      compressed,
      `<< /Type /EmbeddedFile /Subtype /application#aipdf+xml+br /Params << /Size ${Buffer.byteLength(xml)} /CheckSum <${checksum}> >> >>`,
    ),
  ];

  const chunks = [Buffer.from("%PDF-1.7\n%\xE2\xE3\xCF\xD3\n", "binary")];
  const offsets = [0];
  for (let i = 0; i < objects.length; i += 1) {
    offsets.push(Buffer.concat(chunks).length);
    chunks.push(Buffer.from(`${i + 1} 0 obj\n`));
    chunks.push(Buffer.isBuffer(objects[i]) ? objects[i] : Buffer.from(objects[i]));
    chunks.push(Buffer.from("\nendobj\n"));
  }
  const xrefOffset = Buffer.concat(chunks).length;
  chunks.push(Buffer.from(`xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`));
  for (const offset of offsets.slice(1)) {
    chunks.push(Buffer.from(`${String(offset).padStart(10, "0")} 00000 n \n`));
  }
  chunks.push(Buffer.from(`trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xrefOffset}\n%%EOF\n`));
  return Buffer.concat(chunks);
}

function stream(bytes, dict) {
  const prefix = dict.trim().replace(/>>$/, "").trim();
  return Buffer.concat([
    Buffer.from(`${prefix} /Length ${bytes.length} >>\nstream\n`),
    bytes,
    Buffer.from("\nendstream"),
  ]);
}

function visibleContentStream(text) {
  const lines = text.split(/\r?\n/).slice(0, 45);
  return `BT\n/F1 12 Tf\n72 740 Td\n14 TL\n${lines.map((line) => `(${pdfString(line)}) Tj\nT*`).join("\n")}\nET\n`;
}

function xmpMetadata(title, xmlBytes, compressedBytes) {
  return `<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:aipdf="https://aipdf.org/ns/1.0/">
      <dc:title><rdf:Alt><rdf:li xml:lang="x-default">${xmlEscape(title)}</rdf:li></rdf:Alt></dc:title>
      <aipdf:Version>1.0</aipdf:Version>
      <aipdf:SemanticFile>aipdf-semantic.xml.br</aipdf:SemanticFile>
      <aipdf:SemanticEncoding>brotli</aipdf:SemanticEncoding>
      <aipdf:SemanticXmlBytes>${xmlBytes}</aipdf:SemanticXmlBytes>
      <aipdf:SemanticCompressedBytes>${compressedBytes}</aipdf:SemanticCompressedBytes>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>`;
}

function pdfString(input) {
  return input.replace(/\\/g, "\\\\").replace(/\(/g, "\\(").replace(/\)/g, "\\)");
}

function xmlEscape(input) {
  return input.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

