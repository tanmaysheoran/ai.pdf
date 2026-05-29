use crate::{
    render::{build_rendered_pdf, PageOptions},
    security::sanitize_xml,
    xml::validate_xml,
    AipdfError, Result,
};
use crate::font::Font;

mod detect;
mod images;
mod meta;
mod write;
mod xobject;

pub use images::{extract_images, ImageExtract};
pub(crate) use detect::brotli_compress;

const SEMANTIC_FILENAME: &str = "aipdf-semantic.xml.br";
// PDF name for MIME `application/aipdf+xml+br`. The `/` is escaped as `#2F` so
// this is a *conformant* PDF name — an earlier form used a bare `#aipdf`, which
// is an invalid escape that made conformant readers (and lopdf) drop the
// embedded-file object entirely.
const SEMANTIC_SUBTYPE: &str = "/application#2Faipdf+xml+br";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    #[default]
    Minimal,
    Full,
}

#[derive(Clone)]
pub struct BuildOptions {
    pub title: String,
    pub visible_text: Option<String>,
    pub render: RenderMode,
    pub page: PageOptions,
    /// Embedded TrueType font for the visible layer (defaults to DejaVu Sans).
    pub font: Font,
    /// Base directory for resolving relative figure image paths (`full` render).
    pub base_dir: Option<std::path::PathBuf>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            title: "AIPDF Document".to_string(),
            visible_text: None,
            render: RenderMode::Minimal,
            page: PageOptions::default(),
            font: Font::default_font(),
            base_dir: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InspectReport {
    pub is_pdf: bool,
    pub has_semantic_layer: bool,
    pub semantic_compressed_bytes: Option<usize>,
    pub semantic_xml_bytes: Option<usize>,
}

pub fn build_aipdf(xml: &str, options: &BuildOptions) -> Result<Vec<u8>> {
    let xml = sanitize_xml(xml)?;
    validate_xml(&xml)?;
    match options.render {
        // Full render lays out the page first, writes the real page/bbox
        // coordinates back into the XML, then compresses + embeds that payload.
        RenderMode::Full => Ok(build_rendered_pdf(
            &xml,
            &options.title,
            &options.page,
            &options.font,
            options.base_dir.as_deref(),
        )),
        RenderMode::Minimal => {
            let compressed = brotli_compress(xml.as_bytes())?;
            let visible_text = options
                .visible_text
                .clone()
                .unwrap_or_else(|| write::visible_text_from_xml(&xml));
            Ok(write::write_pdf(
                &options.title,
                &visible_text,
                &xml,
                &compressed,
                &options.font,
            ))
        }
    }
}

pub fn inspect_pdf(bytes: &[u8]) -> InspectReport {
    match detect::find_semantic_compressed(bytes) {
        Some(compressed) => {
            let xml = detect::decompress_semantic(&compressed).ok();
            InspectReport {
                is_pdf: bytes.starts_with(b"%PDF-"),
                has_semantic_layer: xml.is_some(),
                semantic_compressed_bytes: Some(compressed.len()),
                semantic_xml_bytes: xml.map(|x| x.len()),
            }
        }
        None => InspectReport {
            is_pdf: bytes.starts_with(b"%PDF-"),
            has_semantic_layer: false,
            semantic_compressed_bytes: None,
            semantic_xml_bytes: None,
        },
    }
}

pub fn extract_semantic_xml(bytes: &[u8]) -> Result<String> {
    let compressed = detect::find_semantic_compressed(bytes).ok_or(AipdfError::SemanticLayerNotFound)?;
    detect::decompress_semantic(&compressed)
}
