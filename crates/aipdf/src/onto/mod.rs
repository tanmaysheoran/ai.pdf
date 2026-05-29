mod build;
mod encode;
mod render_onto;

pub use build::xml_to_onto;

#[derive(Debug, Clone, Default)]
pub(super) struct SectionRecord {
    pub(super) id: String,
    pub(super) level: String,
    pub(super) page: String,
    pub(super) role: String,
    pub(super) title: String,
}

#[derive(Debug, Clone, Default)]
pub(super) struct BlockRecord {
    pub(super) id: String,
    pub(super) kind: String,
    pub(super) section_id: String,
    pub(super) level: String,
    pub(super) page: String,
    pub(super) bbox: String,
    pub(super) role: String,
    pub(super) text: String,
}

#[derive(Debug, Clone, Default)]
pub(super) struct TableRecord {
    pub(super) id: String,
    pub(super) page: String,
    pub(super) bbox: String,
    pub(super) caption: String,
    pub(super) rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct FigureRecord {
    pub(super) id: String,
    pub(super) page: String,
    pub(super) bbox: String,
    pub(super) caption: String,
    pub(super) alt: String,
    pub(super) source: String,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ReferenceRecord {
    pub(super) id: String,
    pub(super) ref_type: String,
    pub(super) text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CaptureKind {
    MetadataTitle,
    Block,
    TableCaption,
    TableCell,
    FigureCaption,
    FigureAltText,
    Reference,
}

#[derive(Debug, Clone)]
pub(super) struct Capture {
    pub(super) kind: CaptureKind,
    pub(super) tag: String,
    pub(super) text: String,
    pub(super) block: Option<BlockRecord>,
    pub(super) reference: Option<ReferenceRecord>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct SectionContext {
    pub(super) id: String,
    pub(super) level: String,
    pub(super) page: String,
}

#[cfg(test)]
mod tests {
    use super::xml_to_onto;

    #[test]
    fn exports_minimal_onto() {
        let onto = xml_to_onto(include_str!("../../../../samples/minimal.xml"));
        assert!(onto.contains("Document[1]:"));
        assert!(onto.contains("Blocks["));
        assert!(onto.contains("Tables[1]:"));
        assert!(onto.contains("Introduction"));
        assert!(onto.contains("Target^Limit|Ideal overhead^<3%"));
    }

    #[test]
    fn exports_maximal_onto() {
        let source = include_str!("../../../../samples/maximal.xml");
        let xml = source
            .split("```xml")
            .nth(1)
            .and_then(|s| s.split("```").next())
            .unwrap_or(source);
        let onto = xml_to_onto(xml);
        assert!(onto.contains("Figures[1]:"));
        assert!(onto.contains("Tables[1]:"));
        assert!(onto.contains("References[2]:"));
        assert!(onto.contains("Mathematical Compression Model"));
        assert!(onto.contains("C = (S_o - S_c) / S_o * 100"));
    }
}
