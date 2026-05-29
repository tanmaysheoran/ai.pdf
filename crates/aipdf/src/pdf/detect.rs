use super::SEMANTIC_FILENAME;
use crate::{AipdfError, Result};
use brotli::{CompressorReader, Decompressor};
use crate::security::sanitize_xml;
use crate::xml::validate_xml;
use std::io::Read;

pub(crate) fn decompress_semantic(compressed: &[u8]) -> Result<String> {
    let mut decompressor = Decompressor::new(compressed, 4096);
    let mut out = String::new();
    decompressor
        .read_to_string(&mut out)
        .map_err(|e| AipdfError::Compression(e.to_string()))?;
    let out = sanitize_xml(&out)?;
    validate_xml(&out)?;
    Ok(out)
}

/// Locate the Brotli-compressed semantic payload. Tries the fast literal
/// byte-scan first (works for files written by this crate's hand builder), then
/// falls back to a structural lopdf lookup that finds the EmbeddedFile by its
/// parsed `/Subtype` regardless of PDF name escaping or xref/object streams —
/// this is what makes ingested (lopdf-saved) files extractable too.
pub(crate) fn find_semantic_compressed(bytes: &[u8]) -> Option<Vec<u8>> {
    if let Some(s) = find_semantic_stream(bytes) {
        return Some(s.to_vec());
    }
    find_semantic_stream_lopdf(bytes)
}

fn find_semantic_stream(bytes: &[u8]) -> Option<&[u8]> {
    use super::SEMANTIC_SUBTYPE;
    // Search for "/Subtype /application#aipdf+xml+br" rather than the bare subtype
    // value so that documents whose text content mentions the marker string don't
    // cause the scanner to land on a rendered-text stream instead of the actual
    // embedded file stream.
    let dict_marker = format!("/Subtype {SEMANTIC_SUBTYPE}");
    let marker_pos = find_bytes(bytes, dict_marker.as_bytes())?;
    let after_marker = &bytes[marker_pos..];
    let stream_rel = find_bytes(after_marker, b"stream\n")?;
    let stream_start = marker_pos + stream_rel + b"stream\n".len();
    let after_stream = &bytes[stream_start..];
    let end_rel = find_bytes(after_stream, b"\nendstream")?;
    Some(&bytes[stream_start..stream_start + end_rel])
}

fn find_semantic_stream_lopdf(bytes: &[u8]) -> Option<Vec<u8>> {
    let doc = lopdf::Document::load_mem(bytes).ok()?;

    // Primary: locate the Filespec for `aipdf-semantic.xml.br` and follow its
    // /EF /F (or /UF) reference to the EmbeddedFile stream. This is robust
    // because it relies on the embedded filename (a plain PDF string) rather
    // than the `/Subtype` name — whose `#` is a non-conformant escape that PDF
    // tools (and lopdf) mangle on re-serialisation.
    for (_id, obj) in doc.objects.iter() {
        let lopdf::Object::Dictionary(d) = obj else {
            continue;
        };
        let filename = d
            .get(b"UF")
            .or_else(|_| d.get(b"F"))
            .ok()
            .and_then(|o| o.as_str().ok());
        let is_semantic_file = filename.map_or(false, |n| {
            n.windows(SEMANTIC_FILENAME.len()).any(|w| w == SEMANTIC_FILENAME.as_bytes())
        });
        if !is_semantic_file {
            continue;
        }
        if let Ok(ef) = d.get(b"EF").and_then(|o| o.as_dict()) {
            let target = ef
                .get(b"F")
                .or_else(|_| ef.get(b"UF"))
                .ok()
                .and_then(|o| o.as_reference().ok());
            if let Some(r) = target {
                if let Ok(lopdf::Object::Stream(s)) = doc.get_object(r) {
                    return Some(s.content.clone());
                }
            }
        }
    }

    // Fallback: any EmbeddedFile stream whose parsed /Subtype mentions "aipdf".
    for (_id, obj) in doc.objects.iter() {
        if let lopdf::Object::Stream(s) = obj {
            let is_ef = s.dict.get(b"Type").ok().and_then(|o| o.as_name().ok())
                == Some(b"EmbeddedFile".as_ref());
            let subtype_is_aipdf = s
                .dict
                .get(b"Subtype")
                .ok()
                .and_then(|o| o.as_name().ok())
                .map_or(false, |n| n.windows(5).any(|w| w == b"aipdf"));
            if is_ef && subtype_is_aipdf {
                return Some(s.content.clone());
            }
        }
    }
    None
}

pub(crate) fn brotli_compress(input: &[u8]) -> Result<Vec<u8>> {
    let mut reader = CompressorReader::new(input, 4096, 6, 22);
    let mut compressed = Vec::new();
    reader
        .read_to_end(&mut compressed)
        .map_err(|e| AipdfError::Compression(e.to_string()))?;
    Ok(compressed)
}

pub(crate) fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}
