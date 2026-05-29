use crate::{
    font::{self, Font, GlyphSet},
    render::{build_rendered_pdf, PageOptions},
    security::sanitize_xml,
    xml::validate_xml,
    AipdfError, Result,
};
use brotli::{CompressorReader, Decompressor};
use flate2::read::ZlibDecoder;
use quick_xml::events::Event;
use quick_xml::Reader;
use sha2::{Digest, Sha256};
use std::io::Read;

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
                .unwrap_or_else(|| visible_text_from_xml(&xml));
            Ok(write_pdf(
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
    match find_semantic_compressed(bytes) {
        Some(compressed) => {
            let xml = decompress_semantic(&compressed).ok();
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
    let compressed = find_semantic_compressed(bytes).ok_or(AipdfError::SemanticLayerNotFound)?;
    decompress_semantic(&compressed)
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
    /// Raw decompressed RGB8 pixel data (row-major, no padding).
    pub raw_rgb8: Vec<u8>,
}

impl ImageExtract {
    /// Save the image to `dir/self.src`, creating parent directories as needed.
    /// The encoder is inferred from the filename extension (`.jpg` → JPEG, `.png` → PNG, etc.).
    pub fn save_to(&self, dir: &std::path::Path) -> Result<std::path::PathBuf> {
        use image::{ImageBuffer, Rgb};
        let dest = dir.join(&self.src);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(AipdfError::Io)?;
        }
        let img = ImageBuffer::<Rgb<u8>, _>::from_raw(self.width, self.height, self.raw_rgb8.clone())
            .ok_or_else(|| AipdfError::InvalidXml("image dimensions do not match pixel data length".into()))?;
        img.save(&dest).map_err(|e| {
            AipdfError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
        })?;
        Ok(dest)
    }
}

/// Extract all raster images embedded in a `--render full` AIPDF file.
///
/// Images are matched positionally: the *n*-th `<image>` element in the
/// semantic XML corresponds to the *n*-th `/Im{n}` XObject in the PDF.
/// If the counts differ (some figures had no source file at build time and
/// were rendered as placeholders), only the matched prefix is returned.
pub fn extract_images(bytes: &[u8]) -> Result<Vec<ImageExtract>> {
    let xml = extract_semantic_xml(bytes)?;
    let xml_images = parse_image_refs(&xml);
    if xml_images.is_empty() {
        return Ok(Vec::new());
    }

    let doc = lopdf::Document::load_mem(bytes).map_err(|e| AipdfError::Pdf(e.to_string()))?;
    let xobjects = collect_image_xobjects(&doc);

    let count = xml_images.len().min(xobjects.len());
    let mut result = Vec::with_capacity(count);
    for i in 0..count {
        let (src, alt) = &xml_images[i];
        let (width, height, ref compressed) = xobjects[i];
        let mut decoder = ZlibDecoder::new(compressed.as_slice());
        let mut raw_rgb8 = Vec::new();
        decoder
            .read_to_end(&mut raw_rgb8)
            .map_err(|e| AipdfError::Compression(e.to_string()))?;
        result.push(ImageExtract {
            src: src.clone(),
            alt: alt.clone(),
            width,
            height,
            raw_rgb8,
        });
    }
    Ok(result)
}

fn parse_image_refs(xml: &str) -> Vec<(String, String)> {
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

fn collect_image_xobjects(doc: &lopdf::Document) -> Vec<(u32, u32, Vec<u8>)> {
    use lopdf::Object;
    use std::collections::BTreeMap;

    let mut seen: BTreeMap<String, (u32, u32, Vec<u8>)> = BTreeMap::new();
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
            let is_image = s
                .dict
                .get(b"Subtype")
                .ok()
                .and_then(|o| o.as_name().ok())
                == Some(b"Image".as_ref());
            let is_flate = s
                .dict
                .get(b"Filter")
                .ok()
                .and_then(|o| o.as_name().ok())
                == Some(b"FlateDecode".as_ref());
            if !is_image || !is_flate {
                continue;
            }
            let Ok(w) = s.dict.get(b"Width").and_then(|o| o.as_i64()) else {
                continue;
            };
            let Ok(h) = s.dict.get(b"Height").and_then(|o| o.as_i64()) else {
                continue;
            };
            seen.insert(name, (w as u32, h as u32, s.content.clone()));
        }
    }

    // Sort by numeric suffix so Im1 < Im2 < Im10 (not lexicographic).
    let mut entries: Vec<(String, (u32, u32, Vec<u8>))> = seen.into_iter().collect();
    entries.sort_by_key(|(name, _)| {
        name.strip_prefix("Im")
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(u32::MAX)
    });
    entries.into_iter().map(|(_, v)| v).collect()
}

fn decompress_semantic(compressed: &[u8]) -> Result<String> {
    let mut decompressor = Decompressor::new(compressed, 4096);
    let mut out = String::new();
    decompressor
        .read_to_string(&mut out)
        .map_err(|e| AipdfError::Compression(e.to_string()))?;
    let out = sanitize_xml(&out)?;
    validate_xml(&out)?;
    Ok(out)
}

/// Locate the Brotli-compressed semantic payload. Tries the fast literal
/// byte-scan first (works for files written by this crate's hand builder), then
/// falls back to a structural lopdf lookup that finds the EmbeddedFile by its
/// parsed `/Subtype` regardless of PDF name escaping or xref/object streams —
/// this is what makes ingested (lopdf-saved) files extractable too.
fn find_semantic_compressed(bytes: &[u8]) -> Option<Vec<u8>> {
    if let Some(s) = find_semantic_stream(bytes) {
        return Some(s.to_vec());
    }
    find_semantic_stream_lopdf(bytes)
}

fn find_semantic_stream(bytes: &[u8]) -> Option<&[u8]> {
    // Search for "/Subtype /application#aipdf+xml+br" rather than the bare subtype
    // value so that documents whose text content mentions the marker string don't
    // cause the scanner to land on a rendered-text stream instead of the actual
    // embedded file stream.
    let dict_marker = format!("/Subtype {SEMANTIC_SUBTYPE}");
    let marker_pos = find_bytes(bytes, dict_marker.as_bytes())?;
    let after_marker = &bytes[marker_pos..];
    let stream_rel = find_bytes(after_marker, b"stream\n")?;
    let stream_start = marker_pos + stream_rel + b"stream\n".len();
    let after_stream = &bytes[stream_start..];
    let end_rel = find_bytes(after_stream, b"\nendstream")?;
    Some(&bytes[stream_start..stream_start + end_rel])
}

fn find_semantic_stream_lopdf(bytes: &[u8]) -> Option<Vec<u8>> {
    let doc = lopdf::Document::load_mem(bytes).ok()?;

    // Primary: locate the Filespec for `aipdf-semantic.xml.br` and follow its
    // /EF /F (or /UF) reference to the EmbeddedFile stream. This is robust
    // because it relies on the embedded filename (a plain PDF string) rather
    // than the `/Subtype` name — whose `#` is a non-conformant escape that PDF
    // tools (and lopdf) mangle on re-serialisation.
    for (_id, obj) in doc.objects.iter() {
        let lopdf::Object::Dictionary(d) = obj else {
            continue;
        };
        let filename = d
            .get(b"UF")
            .or_else(|_| d.get(b"F"))
            .ok()
            .and_then(|o| o.as_str().ok());
        let is_semantic_file = filename.map_or(false, |n| {
            n.windows(SEMANTIC_FILENAME.len()).any(|w| w == SEMANTIC_FILENAME.as_bytes())
        });
        if !is_semantic_file {
            continue;
        }
        if let Ok(ef) = d.get(b"EF").and_then(|o| o.as_dict()) {
            let target = ef
                .get(b"F")
                .or_else(|_| ef.get(b"UF"))
                .ok()
                .and_then(|o| o.as_reference().ok());
            if let Some(r) = target {
                if let Ok(lopdf::Object::Stream(s)) = doc.get_object(r) {
                    return Some(s.content.clone());
                }
            }
        }
    }

    // Fallback: any EmbeddedFile stream whose parsed /Subtype mentions "aipdf".
    for (_id, obj) in doc.objects.iter() {
        if let lopdf::Object::Stream(s) = obj {
            let is_ef = s.dict.get(b"Type").ok().and_then(|o| o.as_name().ok())
                == Some(b"EmbeddedFile".as_ref());
            let subtype_is_aipdf = s
                .dict
                .get(b"Subtype")
                .ok()
                .and_then(|o| o.as_name().ok())
                .map_or(false, |n| n.windows(5).any(|w| w == b"aipdf"));
            if is_ef && subtype_is_aipdf {
                return Some(s.content.clone());
            }
        }
    }
    None
}

pub(crate) fn brotli_compress(input: &[u8]) -> Result<Vec<u8>> {
    let mut reader = CompressorReader::new(input, 4096, 6, 22);
    let mut compressed = Vec::new();
    reader
        .read_to_end(&mut compressed)
        .map_err(|e| AipdfError::Compression(e.to_string()))?;
    Ok(compressed)
}

fn visible_text_from_xml(xml: &str) -> String {
    crate::xml::get_reading_order(xml)
        .map(|blocks| {
            blocks
                .into_iter()
                .filter(|b| matches!(b.kind.as_str(), "title" | "paragraph" | "caption"))
                .map(|b| b.text)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|_| "AIPDF document".to_string())
}

fn write_pdf(title: &str, visible_text: &str, xml: &str, compressed: &[u8], font: &Font) -> Vec<u8> {
    // Encode the visible text as embedded-font glyph IDs so non-ASCII survives.
    let mut glyphs = GlyphSet::new();
    let mut content = String::from("BT\n/F1 12 Tf\n72 740 Td\n14 TL\n");
    for line in visible_text.lines().take(45) {
        let h = glyphs.encode_hex(font, line);
        content.push_str(&format!("<{h}> Tj\nT*\n"));
    }
    content.push_str("ET\n");
    let used = glyphs.used();

    let xmp = xmp_metadata(title, xml.len(), compressed.len());
    let checksum = hex_sha256(compressed);
    let escaped_title = pdf_string(title);
    let producer_note = pdf_string(
        "AIPDF semantic layer present: extract aipdf-semantic.xml.br, Brotli-decompress, parse XML.",
    );

    // Fixed object layout (single page). Font objects 9–13 are wired into the
    // page's /Resources via the Type0 font (object 13).
    let (ff_bytes, len1) = font::font_file2(font);
    let objects: Vec<Vec<u8>> = vec![
        // 1 Catalog
        b"<< /Type /Catalog /Pages 2 0 R /Metadata 6 0 R /Names << /EmbeddedFiles 7 0 R >> /AF [8 0 R] >>".to_vec(),
        // 2 Pages
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        // 3 Page (Type0 font is object 14)
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 14 0 R >> >> /Contents 5 0 R >>".to_vec(),
        // 4 (unused legacy font slot kept to preserve numbering) -> empty dict
        b"<< >>".to_vec(),
        // 5 visible content stream
        stream_object(content.as_bytes(), "<< >>"),
        // 6 XMP metadata
        stream_object(xmp.as_bytes(), "<< /Type /Metadata /Subtype /XML >>"),
        // 7 EmbeddedFiles names
        b"<< /Names [(aipdf-semantic.xml.br) 8 0 R] >>".to_vec(),
        // 8 Filespec
        format!(
            "<< /Type /Filespec /F ({SEMANTIC_FILENAME}) /UF ({SEMANTIC_FILENAME}) /Desc ({escaped_title} semantic XML) /AFRelationship /Data /EF << /F 9 0 R /UF 9 0 R >> >>"
        )
        .into_bytes(),
        // 9 EmbeddedFile (Brotli semantic XML)
        stream_object(
            compressed,
            &format!(
                "<< /Type /EmbeddedFile /Subtype {SEMANTIC_SUBTYPE} /Params << /Size {} /CheckSum <{}> >> >>",
                xml.len(), checksum
            ),
        ),
        // 10 FontFile2
        stream_object(&ff_bytes, &format!("<< /Length1 {len1} /Filter /FlateDecode >>")),
        // 11 FontDescriptor (-> FontFile2 obj 10)
        font::descriptor_dict(font, 10).into_bytes(),
        // 12 CIDFontType2 (-> FontDescriptor obj 11)
        font::cidfont_dict(font, 11, used).into_bytes(),
        // 13 ToUnicode CMap
        stream_object(&font::tounicode_cmap(used), "<< >>"),
        // 14 Type0 (-> CIDFont obj 12, ToUnicode obj 13)
        font::type0_dict(font, 12, 13).into_bytes(),
    ];

    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = vec![0usize];
    for (idx, obj) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", idx + 1).as_bytes());
        pdf.extend_from_slice(obj);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info << /Title ({escaped_title}) /Producer (aipdf prototype) /AIPDFNote ({producer_note}) >> >>\nstartxref\n{}\n%%EOF\n",
            objects.len() + 1,
            xref_offset
        )
        .as_bytes(),
    );
    pdf
}

fn stream_object(bytes: &[u8], dict: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let dict = dict.trim_end_matches(">>").trim();
    out.extend_from_slice(format!("{dict} /Length {} >>\nstream\n", bytes.len()).as_bytes());
    out.extend_from_slice(bytes);
    out.extend_from_slice(b"\nendstream");
    out
}

fn xmp_metadata(title: &str, xml_bytes: usize, compressed_bytes: usize) -> String {
    format!(
        r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about=""
      xmlns:dc="http://purl.org/dc/elements/1.1/"
      xmlns:aipdf="https://aipdf.org/ns/1.0/">
      <dc:title><rdf:Alt><rdf:li xml:lang="x-default">{}</rdf:li></rdf:Alt></dc:title>
      <aipdf:Version>1.0</aipdf:Version>
      <aipdf:SemanticFile>{SEMANTIC_FILENAME}</aipdf:SemanticFile>
      <aipdf:SemanticEncoding>brotli</aipdf:SemanticEncoding>
      <aipdf:SemanticLayerPresent>true</aipdf:SemanticLayerPresent>
      <aipdf:SemanticMimeType>application/aipdf+xml+br</aipdf:SemanticMimeType>
      <aipdf:ContentAuthority>semantic-xml</aipdf:ContentAuthority>
      <aipdf:VisibleRenderingAuthority>pdf-page-content</aipdf:VisibleRenderingAuthority>
      <aipdf:OCRPolicy>skip-when-semantic-layer-present</aipdf:OCRPolicy>
      <aipdf:SemanticXmlBytes>{xml_bytes}</aipdf:SemanticXmlBytes>
      <aipdf:SemanticCompressedBytes>{compressed_bytes}</aipdf:SemanticCompressedBytes>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#,
        xml_escape(title)
    )
}

fn pdf_string(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace('\r', " ")
        .replace('\n', " ")
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}
