import { DISALLOWED_MARKERS } from "./constants.js";
import { AIPDFError } from "./types.js";

export function sanitizeXml(xml: string): string {
  const clean = xml.replace(/^﻿/, "").trim();
  const lowered = clean.toLowerCase();
  for (const marker of DISALLOWED_MARKERS) {
    if (lowered.includes(marker.toLowerCase())) {
      throw new AIPDFError(`disallowed marker \`${marker}\``);
    }
  }
  if (Buffer.byteLength(clean, "utf8") > 16 * 1024 * 1024) {
    throw new AIPDFError("semantic XML exceeds 16 MiB safety limit");
  }
  return clean;
}

export function validateXml(xml: string): void {
  const sanitized = sanitizeXml(xml);
  if (!/^<\?xml[\s\S]*?<document\b|^<document\b/.test(sanitized)) {
    throw new AIPDFError("root element must be <document>");
  }
  if (!/<document\b[^>]*\bversion=["'][^"']+["']/.test(sanitized)) {
    throw new AIPDFError("document version must be present");
  }
  if (!/<section\b[^>]*\bid=["'][^"']+["']/.test(sanitized)) {
    throw new AIPDFError("document must contain at least one section with an id");
  }
  for (const match of sanitized.matchAll(/\bbox=["']([^"']+)["']/g)) {
    if (!/^-?\d+(\.\d+)?,-?\d+(\.\d+)?,-?\d+(\.\d+)?,-?\d+(\.\d+)?$/.test(match[1])) {
      throw new AIPDFError(`invalid bbox: ${match[1]}`);
    }
  }
}
