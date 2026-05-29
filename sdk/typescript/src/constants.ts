// Conformant PDF name for MIME application/aipdf+xml+br ('/' escaped as #2F).
export const SEMANTIC_SUBTYPE = Buffer.from("/application#2Faipdf+xml+br");
// Active-content / structural markers only. Kept in lockstep with the Rust core
// (security.rs) and Python SDK. Natural-language phrases are intentionally NOT
// banned: XML text is data, never instructions, and the visible PDF already
// carries the same words.
export const DISALLOWED_MARKERS = [
  "<!DOCTYPE",
  "<?xml-stylesheet",
  "<?processing",
  "<script",
  "/JavaScript",
  "/Launch",
];
