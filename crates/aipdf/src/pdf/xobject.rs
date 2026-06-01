//! Collect Image XObjects from a parsed PDF and classify their codec,
//! colorspace, and predictor so [`super::images`] can decode them.

use flate2::read::ZlibDecoder;
use std::io::Read;

/// The image codec on an Image XObject — the only two we extract.
#[derive(Debug, Clone, Copy)]
pub(super) enum ImgFilter {
    Flate,
    Dct,
}

/// Coarse colorspace classification sufficient to pick a pixel encoder.
#[derive(Debug, Clone, Copy)]
pub(super) enum ColorKind {
    Rgb,
    Gray,
    /// Paletted: pixel bytes are indices into `RawXObject::palette` (always
    /// expanded to RGB triples before the caller sees the pixels).
    Indexed,
    Other,
}

/// An Image XObject collected from a page, with the metadata needed to decode
/// its stream correctly. `content` is the raw (still filter-encoded) stream.
#[derive(Debug, Clone)]
pub(super) struct RawXObject {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) filter: ImgFilter,
    pub(super) color: ColorKind,
    pub(super) bits: u32,
    pub(super) predictor: Option<i64>,
    pub(super) content: Vec<u8>,
    /// RGB palette for `/Indexed` images: 3 bytes per entry, `(hival+1)`
    /// entries.  `None` for all non-indexed colorspaces.
    pub(super) palette: Option<Vec<u8>>,
}

pub(super) fn collect_image_xobjects(doc: &lopdf::Document) -> Vec<RawXObject> {
    use lopdf::Object;
    use std::collections::BTreeMap;

    let mut seen: BTreeMap<String, RawXObject> = BTreeMap::new();
    for (_page_num, page_id) in doc.get_pages() {
        // lopdf's get_page_resources does not resolve an indirect /Resources
        // reference on the page itself (only inherited parent refs). Resolve
        // the Resources entry manually so both inline and indirect dicts work.
        let resources_dict: &lopdf::Dictionary = {
            let Ok(page_obj) = doc.get_object(page_id) else { continue };
            let Ok(page_dict) = page_obj.as_dict() else { continue };
            let Ok(res_val) = page_dict.get(b"Resources") else { continue };
            let Ok((_, resolved)) = doc.dereference(res_val) else { continue };
            let Ok(d) = resolved.as_dict() else { continue };
            d
        };
        let xobj_raw = match resources_dict.get(b"XObject") {
            Ok(v) => v,
            Err(_) => continue,
        };
        // The /XObject value is often itself an indirect reference to a dict.
        let (_, xobj_deref) = match doc.dereference(xobj_raw) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let Some(xobjects) = xobj_deref.as_dict().ok() else {
            continue;
        };
        for (name_bytes, obj) in xobjects.iter() {
            let name = String::from_utf8_lossy(name_bytes).into_owned();
            if seen.contains_key(&name) {
                continue;
            }
            let Ok(id) = obj.as_reference() else { continue };
            let Ok(Object::Stream(s)) = doc.get_object(id) else {
                continue;
            };
            let is_image = s.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok())
                == Some(b"Image".as_ref());
            if !is_image {
                continue;
            }
            let Some(filter) = s
                .dict
                .get(b"Filter")
                .ok()
                .or_else(|| s.dict.get(b"F").ok())
                .and_then(filter_kind)
            else {
                continue;
            };
            let Ok(w) = s.dict.get(b"Width").and_then(|o| o.as_i64()) else {
                continue;
            };
            let Ok(h) = s.dict.get(b"Height").and_then(|o| o.as_i64()) else {
                continue;
            };
            let bits = s
                .dict
                .get(b"BitsPerComponent")
                .and_then(|o| o.as_i64())
                .unwrap_or(8);
            let cs_obj = s
                .dict
                .get(b"ColorSpace")
                .ok()
                .or_else(|| s.dict.get(b"CS").ok());
            let (color, palette) = cs_obj
                .map(|cs| classify_colorspace(doc, cs))
                .unwrap_or((ColorKind::Other, None));
            let predictor = predictor_of(
                s.dict
                    .get(b"DecodeParms")
                    .ok()
                    .or_else(|| s.dict.get(b"DP").ok()),
            );
            seen.insert(
                name,
                RawXObject {
                    width: w as u32,
                    height: h as u32,
                    filter,
                    color,
                    bits: bits as u32,
                    predictor,
                    content: s.content.clone(),
                    palette,
                },
            );
        }
    }

    // Sort by numeric suffix so Im1 < Im2 < Im10 (not lexicographic).
    let mut entries: Vec<(String, RawXObject)> = seen.into_iter().collect();
    entries.sort_by_key(|(name, _)| {
        name.strip_prefix("Im")
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(u32::MAX)
    });
    entries.into_iter().map(|(_, v)| v).collect()
}

/// Classify the image codec from a `/Filter` value (name, or array whose last
/// entry is the image codec). Returns `None` for codecs we don't extract.
fn filter_kind(obj: &lopdf::Object) -> Option<ImgFilter> {
    use lopdf::Object;
    let name = match obj {
        Object::Name(n) => n.clone(),
        Object::Array(a) => match a.last() {
            Some(Object::Name(n)) => n.clone(),
            _ => return None,
        },
        _ => return None,
    };
    match name.as_slice() {
        b"FlateDecode" | b"Fl" => Some(ImgFilter::Flate),
        b"DCTDecode" | b"DCT" => Some(ImgFilter::Dct),
        _ => None,
    }
}

/// Classify a `/ColorSpace` and optionally extract an RGB palette for
/// `/Indexed` images.  Returns `(kind, palette)` where `palette` is
/// `Some(rgb_bytes)` only when `kind == ColorKind::Indexed`.
fn classify_colorspace(doc: &lopdf::Document, cs: &lopdf::Object) -> (ColorKind, Option<Vec<u8>>) {
    use lopdf::Object;
    let resolved = match cs {
        Object::Reference(r) => match doc.get_object(*r) {
            Ok(o) => o,
            Err(_) => return (ColorKind::Other, None),
        },
        other => other,
    };
    match resolved {
        Object::Name(n) => {
            let kind = match n.as_slice() {
                b"DeviceRGB" | b"CalRGB" | b"RGB" => ColorKind::Rgb,
                b"DeviceGray" | b"CalGray" | b"G" => ColorKind::Gray,
                _ => ColorKind::Other,
            };
            (kind, None)
        }
        Object::Array(a) => {
            let tag = match a.first() {
                Some(Object::Name(n)) => n.as_slice(),
                _ => return (ColorKind::Other, None),
            };
            if tag == b"ICCBased" {
                if let Some(r) = a.get(1).and_then(|o| o.as_reference().ok()) {
                    if let Ok(Object::Stream(s)) = doc.get_object(r) {
                        let kind = match s.dict.get(b"N").and_then(|o| o.as_i64()) {
                            Ok(3) => ColorKind::Rgb,
                            Ok(1) => ColorKind::Gray,
                            _ => ColorKind::Other,
                        };
                        return (kind, None);
                    }
                }
                return (ColorKind::Other, None);
            }
            if tag == b"Indexed" && a.len() >= 4 {
                // [/Indexed base_cs hival lookup]
                // hival is the highest valid index (palette has hival+1 entries).
                if let Ok(hival) = a[2].as_i64() {
                    let n_entries = (hival as usize).saturating_add(1);
                    if let Some(palette) = extract_rgb_palette(doc, &a[3], n_entries) {
                        return (ColorKind::Indexed, Some(palette));
                    }
                }
            }
            (ColorKind::Other, None)
        }
        _ => (ColorKind::Other, None),
    }
}

/// Load the raw palette bytes for a `/Indexed` colorspace lookup table (which
/// may be an inline PDF string or an indirect stream reference), then expand
/// them to a flat RGB buffer (`n_entries × 3` bytes).
///
/// The base colorspace is inferred from `raw.len() / n_entries`:
/// - 3 bytes/entry → RGB, kept as-is.
/// - 1 byte/entry → Gray, expanded to three equal channels.
/// - anything else (e.g. CMYK) → `None` (unsupported).
fn extract_rgb_palette(
    doc: &lopdf::Document,
    lookup: &lopdf::Object,
    n_entries: usize,
) -> Option<Vec<u8>> {
    use lopdf::Object;
    let raw: Vec<u8> = match lookup {
        Object::String(bytes, _) => bytes.clone(),
        Object::Reference(r) => match doc.get_object(*r) {
            Ok(Object::Stream(s)) => decode_stream(&s),
            Ok(Object::String(bytes, _)) => bytes.clone(),
            _ => return None,
        },
        _ => return None,
    };
    if raw.len() < n_entries {
        return None;
    }
    let bpp = raw.len() / n_entries;
    let palette = match bpp {
        3 => raw[..n_entries * 3].to_vec(),
        1 => raw[..n_entries]
            .iter()
            .flat_map(|&g| [g, g, g])
            .collect(),
        _ => return None,
    };
    Some(palette)
}

/// Return the decoded content of a stream, decompressing FlateDecode if needed.
/// Falls back to raw bytes if decompression fails or the filter is unknown.
fn decode_stream(s: &lopdf::Stream) -> Vec<u8> {
    let filter = s
        .dict
        .get(b"Filter")
        .ok()
        .or_else(|| s.dict.get(b"F").ok())
        .and_then(|o| o.as_name().ok());
    match filter {
        Some(f) if f == b"FlateDecode" || f == b"Fl" => {
            let mut decoded = Vec::new();
            if ZlibDecoder::new(s.content.as_slice())
                .read_to_end(&mut decoded)
                .is_ok()
                && !decoded.is_empty()
            {
                return decoded;
            }
            s.content.clone()
        }
        _ => s.content.clone(),
    }
}

/// Read a `/Predictor` from `/DecodeParms` (a dict, or array of dicts).
fn predictor_of(obj: Option<&lopdf::Object>) -> Option<i64> {
    use lopdf::Object;
    let dict = match obj? {
        Object::Dictionary(d) => d,
        Object::Array(a) => a.iter().find_map(|x| x.as_dict().ok())?,
        _ => return None,
    };
    dict.get(b"Predictor").ok().and_then(|p| p.as_i64().ok())
}
