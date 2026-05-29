use super::encode::{encode_array_scalar, encode_scalar};
use super::{BlockRecord, FigureRecord, ReferenceRecord, SectionRecord, TableRecord};

pub(super) fn render_onto(
    version: &str,
    title: &str,
    sections: &[SectionRecord],
    blocks: &[BlockRecord],
    tables: &[TableRecord],
    figures: &[FigureRecord],
    references: &[ReferenceRecord],
) -> String {
    let mut out = String::new();
    out.push_str("Document[1]:\n");
    field(&mut out, "version", version);
    field(&mut out, "title", title);
    field(&mut out, "source_format", "aipdf.semantic.xml");

    out.push('\n');
    out.push_str(&format!("Sections[{}]:\n", sections.len()));
    column(&mut out, "id", sections.iter().map(|s| s.id.as_str()));
    column(&mut out, "level", sections.iter().map(|s| s.level.as_str()));
    column(&mut out, "page", sections.iter().map(|s| s.page.as_str()));
    column(&mut out, "role", sections.iter().map(|s| s.role.as_str()));
    column(&mut out, "title", sections.iter().map(|s| s.title.as_str()));

    out.push('\n');
    out.push_str(&format!("Blocks[{}]:\n", blocks.len()));
    column(&mut out, "id", blocks.iter().map(|b| b.id.as_str()));
    column(&mut out, "kind", blocks.iter().map(|b| b.kind.as_str()));
    column(
        &mut out,
        "section_id",
        blocks.iter().map(|b| b.section_id.as_str()),
    );
    column(&mut out, "level", blocks.iter().map(|b| b.level.as_str()));
    column(&mut out, "page", blocks.iter().map(|b| b.page.as_str()));
    column(&mut out, "bbox", blocks.iter().map(|b| b.bbox.as_str()));
    column(&mut out, "role", blocks.iter().map(|b| b.role.as_str()));
    column(&mut out, "text", blocks.iter().map(|b| b.text.as_str()));

    out.push('\n');
    out.push_str(&format!("Tables[{}]:\n", tables.len()));
    column(&mut out, "id", tables.iter().map(|t| t.id.as_str()));
    column(&mut out, "page", tables.iter().map(|t| t.page.as_str()));
    column(&mut out, "bbox", tables.iter().map(|t| t.bbox.as_str()));
    column(
        &mut out,
        "caption",
        tables.iter().map(|t| t.caption.as_str()),
    );
    column_raw(
        &mut out,
        "rows",
        tables.iter().map(|t| t.rows_as_onto()).collect::<Vec<_>>(),
    );

    out.push('\n');
    out.push_str(&format!("Figures[{}]:\n", figures.len()));
    column(&mut out, "id", figures.iter().map(|f| f.id.as_str()));
    column(&mut out, "page", figures.iter().map(|f| f.page.as_str()));
    column(&mut out, "bbox", figures.iter().map(|f| f.bbox.as_str()));
    column(
        &mut out,
        "caption",
        figures.iter().map(|f| f.caption.as_str()),
    );
    column(&mut out, "alt", figures.iter().map(|f| f.alt.as_str()));
    column(
        &mut out,
        "source",
        figures.iter().map(|f| f.source.as_str()),
    );

    out.push('\n');
    out.push_str(&format!("References[{}]:\n", references.len()));
    column(&mut out, "id", references.iter().map(|r| r.id.as_str()));
    column(
        &mut out,
        "type",
        references.iter().map(|r| r.ref_type.as_str()),
    );
    column(&mut out, "text", references.iter().map(|r| r.text.as_str()));

    out.trim_end().to_string()
}

impl TableRecord {
    pub(super) fn rows_as_onto(&self) -> String {
        self.rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| encode_array_scalar(cell))
                    .collect::<Vec<_>>()
                    .join("^")
            })
            .collect::<Vec<_>>()
            .join("|")
    }
}

fn field(out: &mut String, name: &str, value: &str) {
    out.push_str("    ");
    out.push_str(name);
    out.push_str(": ");
    out.push_str(&encode_scalar(value));
    out.push('\n');
}

fn column<'a, I, S>(out: &mut String, name: &str, values: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str> + 'a,
{
    out.push_str("    ");
    out.push_str(name);
    out.push_str(": ");
    out.push_str(
        &values
            .into_iter()
            .map(|v| encode_scalar(v.as_ref()))
            .collect::<Vec<_>>()
            .join("|"),
    );
    out.push('\n');
}

fn column_raw(out: &mut String, name: &str, values: Vec<String>) {
    out.push_str("    ");
    out.push_str(name);
    out.push_str(": ");
    out.push_str(&values.join("|"));
    out.push('\n');
}
