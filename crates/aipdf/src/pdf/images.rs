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
    if xml_images.is_empty() {
        return Ok(Vec::new());
    }

    // De-duplicate by src, preserving first-seen order.
    let mut unique: Vec<(String, String)> = Vec::new();
    let mut seen_src = std::collections::HashSet::new();
    for (src, alt) in xml_images {
        if seen_src.insert(src.clone()) {
            unique.push((src, alt));
        }
    }

    let doc = lopdf::Document::load_mem(bytes).map_err(|e| AipdfError::Pdf(e.to_string()))?;
    let xobjects = collect_image_xobjects(&doc);

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
            // We don't undo PNG/TIFF predictors; refuse rather than corrupt.
            if matches!(xo.predictor, Some(p) if p >= 2) {
                return None;
            }
            let mut raw = Vec::new();
            ZlibDecoder::new(xo.content.as_slice())
                .read_to_end(&mut raw)
                .ok()?;
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
                ColorKind::Other => None,
            }
        }
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
