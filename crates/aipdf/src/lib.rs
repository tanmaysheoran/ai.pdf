pub(crate) mod font;
mod ingest;
mod markdown;
mod onto;
mod pdf;
pub(crate) mod render;
mod security;
mod source;
mod xml;

pub use font::Font;
pub use ingest::{ingest_pdf, IngestOptions, OcrMode};
pub use markdown::{xml_to_markdown, xml_to_markdown_ast_json};
pub use onto::xml_to_onto;
pub use pdf::{
    build_aipdf, extract_semantic_xml, inspect_pdf, BuildOptions, InspectReport, RenderMode,
};
pub use render::PageOptions;
pub use security::sanitize_xml;
pub use source::{semantic_xml_from_source, SourceKind};
pub use xml::{
    find_citations, get_reading_order, get_tables, validate_xml, SemanticBlock,
    SUPPORTED_MAJOR_VERSION,
};

use std::fs;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum AipdfError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid semantic XML: {0}")]
    InvalidXml(String),
    #[error("semantic layer not found")]
    SemanticLayerNotFound,
    #[error("compression error: {0}")]
    Compression(String),
    #[error("PDF parse error: {0}")]
    Pdf(String),
}

pub type Result<T> = std::result::Result<T, AipdfError>;

#[derive(Debug, Clone)]
pub struct AipdfDocument {
    path: Option<std::path::PathBuf>,
    xml: Option<String>,
}

impl AipdfDocument {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = fs::read(path.as_ref())?;
        let xml = extract_semantic_xml(&bytes).ok();
        Ok(Self {
            path: Some(path.as_ref().to_path_buf()),
            xml,
        })
    }

    pub fn from_xml(xml: impl Into<String>) -> Result<Self> {
        let xml = sanitize_xml(&xml.into())?;
        validate_xml(&xml)?;
        Ok(Self {
            path: None,
            xml: Some(xml),
        })
    }

    pub fn has_semantic_layer(&self) -> bool {
        self.xml.is_some()
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn to_xml(&self) -> Result<&str> {
        self.xml.as_deref().ok_or(AipdfError::SemanticLayerNotFound)
    }

    pub fn to_markdown(&self) -> Result<String> {
        Ok(xml_to_markdown(self.to_xml()?))
    }

    pub fn to_onto(&self) -> Result<String> {
        Ok(xml_to_onto(self.to_xml()?))
    }

    pub fn get_structure(&self) -> Result<Vec<SemanticBlock>> {
        get_reading_order(self.to_xml()?)
    }

    pub fn get_tables(&self) -> Result<Vec<String>> {
        get_tables(self.to_xml()?)
    }

    pub fn get_reading_order(&self) -> Result<Vec<SemanticBlock>> {
        get_reading_order(self.to_xml()?)
    }

    pub fn find_citations(&self) -> Result<Vec<String>> {
        find_citations(self.to_xml()?)
    }
}
