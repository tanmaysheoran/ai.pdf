use super::assemble::{Assembler, hex_sha256, pdf_str, stream_obj, xmp_metadata};
use super::layout::Layout;
use super::options::PageOptions;
use super::parse::{BlockCoord, DocElem, parse_elements};
use super::{SEMANTIC_FILENAME, SEMANTIC_SUBTYPE};
use crate::font::{self, Font};
use quick_xml::events::Event;
use quick_xml::{Reader, Writer};
use std::path::Path;

fn elem_id(e: &DocElem) -> Option<&str> {
    match e {
        DocElem::Heading { id, .. }
        | DocElem::Paragraph { id, .. }
        | DocElem::Table { id, .. }
        | DocElem::CodeBlock { id, .. }
        | DocElem::List { id, .. }
        | DocElem::Citation { id, .. }
        | DocElem::Note { id, .. }
        | DocElem::Figure { id, .. } => id.as_deref(),
        DocElem::DocTitle(_) => None,
    }
}

pub fn build_rendered_pdf(
    xml: &str,
    title: &str,
    page_opts: &PageOptions,
    font: &Font,
    base_dir: Option<&Path>,
) -> Vec<u8> {
    let elems = parse_elements(xml);
    let mut layout = Layout::new(page_opts.clone(), font.clone(), base_dir.map(Path::to_path_buf));

    for elem in &elems {
        let id = elem_id(elem).map(str::to_string);
        let top_before = layout.cursor_y;
        let page_before = layout.page_num;

        match elem {
            DocElem::DocTitle(t) => layout.render_doc_title(t),
            DocElem::Heading { level, text, .. } => layout.render_heading(*level, text),
            DocElem::Paragraph { text, .. } => layout.render_paragraph(text),
            DocElem::Citation { text, .. } => layout.render_citation(text),
            DocElem::Note { text, .. } => layout.render_note(text),
            DocElem::CodeBlock { language, text, .. } => {
                layout.render_code_block(text, language.as_deref())
            }
            DocElem::List { ordered, items, .. } => layout.render_list(*ordered, items),
            DocElem::Table { caption, rows, .. } => layout.render_table(caption.as_deref(), rows),
            DocElem::Figure { alt, src, caption, .. } => {
                layout.render_figure(alt, src, caption.as_deref())
            }
        }

        // Record the block's page + bbox. If a page break happened mid-block we
        // attribute it to the page it ended up on (an approximation for blocks
        // that span a boundary); the common single-page case is exact.
        if let Some(id) = id {
            let page_after = layout.page_num;
            let (page, top) = if page_after == page_before {
                (page_before, top_before)
            } else {
                (page_after, layout.opts.height - layout.opts.margin_top)
            };
            let x0 = layout.opts.margin_left;
            let x1 = x0 + layout.opts.content_width();
            let bottom = layout.cursor_y.max(layout.opts.margin_bottom);
            layout.coords.push(BlockCoord {
                id,
                page,
                bbox: (x0, bottom, x1, top),
            });
        }
    }

    let (page_streams, page_count, glyphs, images, coords) = layout.finalize();

    // Rewrite the semantic XML with the real page/bbox coordinates, then embed
    // that updated payload (compressed) so the machine layer matches the visuals.
    let xml = apply_coordinates(xml, &coords);
    let xml = xml.as_str();
    let compressed = crate::pdf::brotli_compress(xml.as_bytes()).expect("brotli compress");
    let compressed = compressed.as_slice();

    // Build PDF object tree
    let mut asm = Assembler::new();
    let catalog_id = asm.reserve(); // 1
    let pages_id = asm.reserve(); // 2

    // Embedded CID/Type0 font (referenced as /F1 by every page).
    let used = glyphs.used();
    let (ff_bytes, len1) = font::font_file2(font);
    let ff_id = asm.add(stream_obj(
        &ff_bytes,
        &format!("<< /Length1 {len1} /Filter /FlateDecode >>"),
    ));
    let desc_id = asm.add(font::descriptor_dict(font, ff_id).into_bytes());
    let cid_id = asm.add(font::cidfont_dict(font, desc_id, used).into_bytes());
    let tu_id = asm.add(stream_obj(&font::tounicode_cmap(used), "<< >>"));
    let type0_id = asm.add(font::type0_dict(font, cid_id, tu_id).into_bytes());

    // Embed figure images as shared XObjects (one resource dict for all pages).
    let mut xobject_entries = Vec::new();
    for img in &images {
        let dict = format!(
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode >>",
            img.enc.width, img.enc.height
        );
        let id = asm.add(stream_obj(&img.enc.data, &dict));
        xobject_entries.push(format!("/{} {id} 0 R", img.name));
    }
    let resources = if xobject_entries.is_empty() {
        format!("<< /Font << /F1 {type0_id} 0 R >> >>")
    } else {
        format!(
            "<< /Font << /F1 {type0_id} 0 R >> /XObject << {} >> >>",
            xobject_entries.join(" ")
        )
    };

    // Page content streams and page objects
    let mut page_ids = Vec::new();
    for stream in &page_streams {
        let cs_id = asm.add(stream_obj(stream.as_bytes(), "<< >>"));
        let pg_id = asm.add(
            format!(
                "<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 {} {}] /Resources {resources} /Contents {cs_id} 0 R >>",
                page_opts.width, page_opts.height
            )
            .into_bytes(),
        );
        page_ids.push(pg_id);
    }

    // XMP metadata
    let xmp = xmp_metadata(title, xml.len(), compressed.len());
    let xmp_id = asm.add(stream_obj(xmp.as_bytes(), "<< /Type /Metadata /Subtype /XML >>"));

    // Semantic layer
    let checksum = hex_sha256(compressed);
    let esc_title = pdf_str(title);
    let ef_dict = format!(
        "<< /Type /EmbeddedFile /Subtype {SEMANTIC_SUBTYPE} /Params << /Size {} /CheckSum <{checksum}> >> >>",
        xml.len()
    );
    let ef_id = asm.add(stream_obj(compressed, &ef_dict));
    let filespec_id = asm.add(
        format!(
            "<< /Type /Filespec /F ({SEMANTIC_FILENAME}) /UF ({SEMANTIC_FILENAME}) /Desc ({esc_title} semantic XML) /AFRelationship /Data /EF << /F {ef_id} 0 R /UF {ef_id} 0 R >> >>"
        )
        .into_bytes(),
    );
    let names_id = asm.add(
        format!("<< /Names [(aipdf-semantic.xml.br) {filespec_id} 0 R] >>").into_bytes(),
    );

    // Fill in Pages object
    let kids = page_ids
        .iter()
        .map(|id| format!("{id} 0 R"))
        .collect::<Vec<_>>()
        .join(" ");
    asm.set(
        pages_id,
        format!("<< /Type /Pages /Kids [{kids}] /Count {page_count} >>").into_bytes(),
    );

    // Fill in Catalog
    asm.set(
        catalog_id,
        format!(
            "<< /Type /Catalog /Pages {pages_id} 0 R /Metadata {xmp_id} 0 R /Names << /EmbeddedFiles {names_id} 0 R >> /AF [{filespec_id} 0 R] >>"
        )
        .into_bytes(),
    );

    asm.build(catalog_id, title)
}

// ── Coordinate write-back ──────────────────────────────────────────────────────

/// Rewrite the semantic XML, replacing each matched element's `page` and `bbox`
/// attributes with the real laid-out coordinates. Elements without a recorded
/// coordinate are passed through unchanged.
fn apply_coordinates(xml: &str, coords: &[BlockCoord]) -> String {
    use quick_xml::events::BytesStart;
    use std::collections::HashMap;
    use std::io::Cursor;

    let map: HashMap<&str, &BlockCoord> = coords.iter().map(|c| (c.id.as_str(), c)).collect();

    let rewrite = |e: &BytesStart| -> Option<BytesStart<'static>> {
        let id = e
            .attributes()
            .flatten()
            .find(|a| a.key.as_ref() == b"id")
            .map(|a| String::from_utf8_lossy(a.value.as_ref()).into_owned())?;
        let c = map.get(id.as_str())?;
        let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
        let mut nb = BytesStart::new(name);
        for attr in e.attributes().flatten() {
            let key = attr.key.as_ref();
            if key == b"page" || key == b"bbox" {
                continue; // replaced below
            }
            let k = String::from_utf8_lossy(key).into_owned();
            let v = String::from_utf8_lossy(attr.value.as_ref()).into_owned();
            nb.push_attribute((k.as_str(), v.as_str()));
        }
        let (x0, y0, x1, y1) = c.bbox;
        nb.push_attribute(("page", c.page.to_string().as_str()));
        nb.push_attribute(("bbox", format!("{x0:.1},{y0:.1},{x1:.1},{y1:.1}").as_str()));
        Some(nb)
    };

    let mut reader = Reader::from_str(xml);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let ev = rewrite(&e);
                match ev {
                    Some(nb) => writer.write_event(Event::Start(nb)),
                    None => writer.write_event(Event::Start(e)),
                }
                .ok();
            }
            Ok(Event::Empty(e)) => {
                let ev = rewrite(&e);
                match ev {
                    Some(nb) => writer.write_event(Event::Empty(nb)),
                    None => writer.write_event(Event::Empty(e)),
                }
                .ok();
            }
            Ok(Event::Eof) => break,
            Ok(ev) => {
                writer.write_event(ev).ok();
            }
            Err(_) => break,
        }
    }
    String::from_utf8(writer.into_inner().into_inner()).unwrap_or_else(|_| xml.to_string())
}
