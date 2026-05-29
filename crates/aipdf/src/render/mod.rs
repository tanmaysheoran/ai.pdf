pub(crate) const SEMANTIC_FILENAME: &str = "aipdf-semantic.xml.br";
// Conformant PDF name for MIME `application/aipdf+xml+br` (`/` escaped as `#2F`).
pub(crate) const SEMANTIC_SUBTYPE: &str = "/application#2Faipdf+xml+br";

mod assemble;
mod build;
mod image;
mod layout;
mod layout_blocks;
mod options;
mod parse;

pub use options::PageOptions;
pub(crate) use build::build_rendered_pdf;
