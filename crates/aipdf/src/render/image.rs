use crate::font;
use std::path::{Path, PathBuf};

/// A decoded raster image ready to embed as a `/DeviceRGB` `/FlateDecode` XObject.
pub(super) struct EncodedImage {
    pub(super) width: u32,
    pub(super) height: u32,
    /// zlib-compressed 8-bit RGB samples (row-major, w*h*3 bytes uncompressed).
    pub(super) data: Vec<u8>,
}

/// An image XObject placed on a page, named `/Im{n}` in the content stream.
pub(super) struct ImageObj {
    pub(super) name: String,
    pub(super) enc: EncodedImage,
}

/// Decode an image file to RGB8 (alpha dropped) and zlib-compress its samples.
/// Returns None for missing/unreadable/remote sources so the caller can fall
/// back to a labelled placeholder.
pub(super) fn load_image(base_dir: Option<&Path>, src: &str) -> Option<EncodedImage> {
    if src.is_empty() || src.starts_with("http://") || src.starts_with("https://") {
        return None; // the semantic layer stores no network references
    }
    let path = match base_dir {
        Some(dir) => dir.join(src),
        None => PathBuf::from(src),
    };
    let img = image::open(&path).ok()?.to_rgb8();
    let (width, height) = img.dimensions();
    Some(EncodedImage {
        width,
        height,
        data: font::flate(&img.into_raw()),
    })
}
