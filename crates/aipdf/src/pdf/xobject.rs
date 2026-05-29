//! Collect Image XObjects from a parsed PDF and classify their codec,
//! colorspace, and predictor so [`super::images`] can decode them.

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
}

pub(super) fn collect_image_xobjects(doc: &lopdf::Document) -> Vec<RawXObject> {
    use lopdf::Object;
    use std::collections::BTreeMap;

    let mut seen: BTreeMap<String, RawXObject> = BTreeMap::new();
    for (_page_num, page_id) in doc.get_pages() {
        let Ok((Some(resources), _)) = doc.get_page_resources(page_id) else {
            continue;
        };
        let Some(xobjects) = resources
            .get(b"XObject")
            .ok()
            .and_then(|o| o.as_dict().ok())
        else {
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
            let color = s
                .dict
                .get(b"ColorSpace")
                .ok()
                .or_else(|| s.dict.get(b"CS").ok())
                .map(|cs| color_kind(doc, cs))
                .unwrap_or(ColorKind::Other);
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

/// Classify a `/ColorSpace` into RGB / Gray / Other, resolving references and
/// `[/ICCBased <stream>]` (by the stream's `/N` component count).
fn color_kind(doc: &lopdf::Document, cs: &lopdf::Object) -> ColorKind {
    use lopdf::Object;
    let resolved = match cs {
        Object::Reference(r) => match doc.get_object(*r) {
            Ok(o) => o,
            Err(_) => return ColorKind::Other,
        },
        other => other,
    };
    match resolved {
        Object::Name(n) => match n.as_slice() {
            b"DeviceRGB" | b"CalRGB" | b"RGB" => ColorKind::Rgb,
            b"DeviceGray" | b"CalGray" | b"G" => ColorKind::Gray,
            _ => ColorKind::Other,
        },
        Object::Array(a) => {
            if let Some(Object::Name(n)) = a.first() {
                if n.as_slice() == b"ICCBased" {
                    if let Some(r) = a.get(1).and_then(|o| o.as_reference().ok()) {
                        if let Ok(Object::Stream(s)) = doc.get_object(r) {
                            return match s.dict.get(b"N").and_then(|o| o.as_i64()) {
                                Ok(3) => ColorKind::Rgb,
                                Ok(1) => ColorKind::Gray,
                                _ => ColorKind::Other,
                            };
                        }
                    }
                }
            }
            ColorKind::Other
        }
        _ => ColorKind::Other,
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
