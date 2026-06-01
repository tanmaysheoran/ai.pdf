use crate::{AipdfError, Result};
use flate2::read::ZlibDecoder;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::Read;

use super::detect::find_semantic_compressed;
use super::xobject::{collect_image_xobjects, ColorKind, ImgFilter, RawXObject};

/// The pixel data backing an [`ImageExtract`], in whatever form the source
/// XObject provided it. RGB/Gray variants are decoded raw samples (8 bits per
/// component, row-major, no padding); `Jpeg` is an already-encoded DCTDecode
/// stream written through verbatim with no re-encoding.
#[derive(Debug, Clone)]
pub enum ImagePayload {
    Rgb8(Vec<u8>),
    Gray8(Vec<u8>),
    Jpeg(Vec<u8>),
}

/// A raster image extracted from the PDF's embedded XObjects, correlated with
/// its original `src` and `alt` attributes from the semantic XML.
#[derive(Debug, Clone)]
pub struct ImageExtract {
    /// Original `src` attribute from the semantic XML (e.g. `"1.jpg"`).
    pub src: String,
    /// `alt` attribute from the semantic XML.
    pub alt: String,
    pub width: u32,
    pub height: u32,
    /// Decoded pixels (or passthrough JPEG bytes) for this image.
    pub payload: ImagePayload,
}

impl ImageExtract {
    /// Save the image to `dir/self.src`, creating parent directories as needed.
    /// Decoded RGB/Gray pixels are encoded from the filename extension (`.jpg`
    /// → JPEG, `.png` → PNG, etc.); `Jpeg` payloads are written verbatim.
    pub fn save_to(&self, dir: &std::path::Path) -> Result<std::path::PathBuf> {
        use image::{ImageBuffer, Luma, Rgb};
        let dest = dir.join(&self.src);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(AipdfError::Io)?;
        }
        let dim_err =
            || AipdfError::InvalidXml("image dimensions do not match pixel data length".into());
        let enc_err = |e: image::ImageError| {
            AipdfError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
        };
        match &self.payload {
            // Already-encoded JPEG from a DCTDecode stream — the bytes are a
            // complete JPEG file, so write them straight through (no re-encode).
            ImagePayload::Jpeg(bytes) => std::fs::write(&dest, bytes).map_err(AipdfError::Io)?,
            ImagePayload::Rgb8(px) => {
                let img = ImageBuffer::<Rgb<u8>, _>::from_raw(self.width, self.height, px.clone())
                    .ok_or_else(dim_err)?;
                img.save(&dest).map_err(enc_err)?;
            }
            ImagePayload::Gray8(px) => {
                let img = ImageBuffer::<Luma<u8>, _>::from_raw(self.width, self.height, px.clone())
                    .ok_or_else(dim_err)?;
                img.save(&dest).map_err(enc_err)?;
            }
        }
        Ok(dest)
    }
}

/// Extract raster images embedded in an AIPDF file (both `--render full` and
/// `--render browser`, which Chrome emits as the same `/Im{n}` Image XObjects).
///
/// Image refs are de-duplicated by `src` first: two figures pointing at the
/// same file (e.g. an `<img>` and a CSS background of `1.jpg`) yield one
/// extracted file rather than overwriting each other on disk. Each unique src
/// is then matched positionally to the *n*-th sorted XObject — the best
/// heuristic available, since a Chrome PDF carries no `<img>`→XObject mapping.
/// If the counts differ, only the matched prefix is returned; images whose
/// encoding we cannot decode safely are skipped with a note instead of being
/// written corrupt.
pub fn extract_images(bytes: &[u8]) -> Result<Vec<ImageExtract>> {
    use super::detect::decompress_semantic;

    let compressed = find_semantic_compressed(bytes).ok_or(AipdfError::SemanticLayerNotFound)?;
    let xml = decompress_semantic(&compressed)?;
    let xml_images = parse_image_refs(&xml);

    let doc = lopdf::Document::load_mem(bytes).map_err(|e| AipdfError::Pdf(e.to_string()))?;
    let xobjects = collect_image_xobjects(&doc);

    if xml_images.is_empty() {
        // Ingested PDF: semantic XML has no <image> refs, but XObjects may exist.
        // Auto-name images by index so callers get them without needing semantic refs.
        let mut result = Vec::new();
        for (i, xo) in xobjects.iter().enumerate() {
            let ext = match xo.filter {
                ImgFilter::Dct => "jpg",
                ImgFilter::Flate => "png",
            };
            let src = format!("image-{:03}.{ext}", i + 1);
            match decode_xobject(xo) {
                Some(payload) => result.push(ImageExtract {
                    src,
                    alt: String::new(),
                    width: xo.width,
                    height: xo.height,
                    payload,
                }),
                None => eprintln!(
                    "note: skipping image '{src}' — unsupported XObject encoding"
                ),
            }
        }
        return Ok(result);
    }

    // Built PDF: de-duplicate refs by src, preserving first-seen order, then match
    // positionally to the n-th sorted XObject.
    let mut unique: Vec<(String, String)> = Vec::new();
    let mut seen_src = std::collections::HashSet::new();
    for (src, alt) in xml_images {
        if seen_src.insert(src.clone()) {
            unique.push((src, alt));
        }
    }

    let count = unique.len().min(xobjects.len());
    let mut result = Vec::with_capacity(count);
    for i in 0..count {
        let (src, alt) = &unique[i];
        let xo = &xobjects[i];
        match decode_xobject(xo) {
            Some(payload) => result.push(ImageExtract {
                src: src.clone(),
                alt: alt.clone(),
                width: xo.width,
                height: xo.height,
                payload,
            }),
            None => eprintln!(
                "note: skipping image '{src}' — unsupported XObject encoding (colorspace/bit-depth/predictor not decodable)"
            ),
        }
    }
    Ok(result)
}

/// Decode a collected XObject into a savable payload, or `None` if its encoding
/// is one we don't handle (so the caller can skip rather than emit garbage).
fn decode_xobject(xo: &RawXObject) -> Option<ImagePayload> {
    match xo.filter {
        // DCTDecode streams are complete JPEG files — pass through untouched.
        ImgFilter::Dct => Some(ImagePayload::Jpeg(xo.content.clone())),
        ImgFilter::Flate => {
            if xo.bits != 8 {
                return None;
            }
            let mut raw = Vec::new();
            ZlibDecoder::new(xo.content.as_slice())
                .read_to_end(&mut raw)
                .ok()?;
            // Undo TIFF (predictor 2) or PNG (predictors 10–15) row filtering
            // before interpreting the decompressed bytes as pixel data.
            if let Some(p) = xo.predictor {
                if p >= 2 && !undo_predictor(&mut raw, p, xo.width, xo.color, xo.bits) {
                    return None;
                }
            }
            let px = xo.width as usize * xo.height as usize;
            match xo.color {
                ColorKind::Rgb => {
                    let need = px.checked_mul(3)?;
                    if raw.len() < need {
                        return None;
                    }
                    raw.truncate(need);
                    Some(ImagePayload::Rgb8(raw))
                }
                ColorKind::Gray => {
                    if raw.len() < px {
                        return None;
                    }
                    raw.truncate(px);
                    Some(ImagePayload::Gray8(raw))
                }
                ColorKind::Indexed => {
                    // Each byte is a palette index; expand to RGB using the
                    // palette stored alongside this XObject.
                    let palette = xo.palette.as_ref()?;
                    let n_entries = palette.len() / 3;
                    if n_entries == 0 || raw.len() < px {
                        return None;
                    }
                    let mut rgb = Vec::with_capacity(px * 3);
                    for &idx in &raw[..px] {
                        let i = (idx as usize).min(n_entries - 1);
                        rgb.extend_from_slice(&palette[i * 3..i * 3 + 3]);
                    }
                    Some(ImagePayload::Rgb8(rgb))
                }
                ColorKind::Other => None,
            }
        }
    }
}

/// Un-filter a FlateDecode image stream in-place after Zlib decompression.
///
/// Predictor 2 — TIFF horizontal differencing: rows are flat `width × bpp`
/// byte runs; un-diff each row left-to-right, no per-row header byte.
///
/// Predictors 10–15 — PNG row-based filtering: each row starts with a filter
/// type byte (0 None / 1 Sub / 2 Up / 3 Average / 4 Paeth) followed by
/// `width × bpp` sample bytes. Reconstruct using the prior row.
///
/// Returns `false` when dimensions are inconsistent with the data length or the
/// colour kind is unsupported; `raw` is then left in an unspecified state.
fn undo_predictor(raw: &mut Vec<u8>, predictor: i64, width: u32, color: ColorKind, bits: u32) -> bool {
    if bits != 8 {
        return false;
    }
    let bpp: usize = match color {
        ColorKind::Rgb => 3,
        ColorKind::Gray | ColorKind::Indexed => 1, // index bytes are 1 byte per pixel
        ColorKind::Other => return false,
    };
    match predictor {
        2 => {
            let row_len = width as usize * bpp;
            if row_len == 0 || raw.len() % row_len != 0 {
                return false;
            }
            for row in raw.chunks_mut(row_len) {
                for i in bpp..row.len() {
                    row[i] = row[i].wrapping_add(row[i - bpp]);
                }
            }
            true
        }
        10..=15 => {
            let row_len = width as usize * bpp;
            let stride = row_len + 1; // +1 for the per-row filter-type byte
            if stride == 1 || raw.len() % stride != 0 {
                return false;
            }
            let nrows = raw.len() / stride;
            let mut out = vec![0u8; row_len * nrows];
            let mut prev = vec![0u8; row_len];
            for y in 0..nrows {
                let base = y * stride;
                let filter = raw[base];
                let src = &raw[base + 1..base + stride];
                let dst = &mut out[y * row_len..(y + 1) * row_len];
                apply_png_row(filter, src, &prev, dst, bpp);
                prev.copy_from_slice(dst);
            }
            *raw = out;
            true
        }
        _ => false,
    }
}

#[inline]
fn apply_png_row(filter: u8, src: &[u8], prev: &[u8], dst: &mut [u8], bpp: usize) {
    let n = dst.len();
    match filter {
        0 => dst.copy_from_slice(src),
        1 => {
            for i in 0..n {
                let a = if i >= bpp { dst[i - bpp] } else { 0 };
                dst[i] = src[i].wrapping_add(a);
            }
        }
        2 => {
            for i in 0..n {
                dst[i] = src[i].wrapping_add(prev[i]);
            }
        }
        3 => {
            for i in 0..n {
                let a = if i >= bpp { dst[i - bpp] as u16 } else { 0 };
                let b = prev[i] as u16;
                dst[i] = src[i].wrapping_add(((a + b) >> 1) as u8);
            }
        }
        4 => {
            for i in 0..n {
                let a = if i >= bpp { dst[i - bpp] as i32 } else { 0 };
                let b = prev[i] as i32;
                let c = if i >= bpp { prev[i - bpp] as i32 } else { 0 };
                let p = a + b - c;
                let pr = if (p - a).abs() <= (p - b).abs() && (p - a).abs() <= (p - c).abs() {
                    a
                } else if (p - b).abs() <= (p - c).abs() {
                    b
                } else {
                    c
                };
                dst[i] = src[i].wrapping_add(pr as u8);
            }
        }
        // Unknown filter type: treat as None rather than emitting corrupt pixels.
        _ => dst.copy_from_slice(src),
    }
}

pub(super) fn parse_image_refs(xml: &str) -> Vec<(String, String)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut images = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Empty(e)) if e.name().as_ref() == b"image" => {
                let mut src = String::new();
                let mut alt = String::new();
                for attr in e.attributes().flatten() {
                    match attr.key.as_ref() {
                        b"src" => src = String::from_utf8_lossy(attr.value.as_ref()).into_owned(),
                        b"alt" => alt = String::from_utf8_lossy(attr.value.as_ref()).into_owned(),
                        _ => {}
                    }
                }
                if !src.is_empty() {
                    images.push((src, alt));
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    images
}
