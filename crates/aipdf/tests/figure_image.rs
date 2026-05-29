//! Verifies that <figure><image src=..> embeds a real raster XObject in the
//! visible PDF (full render), and falls back to a placeholder when the source
//! cannot be loaded.

use aipdf::{build_aipdf, BuildOptions, RenderMode};
use lopdf::Object;

fn doc_with_figure(src: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<document version="1.0" id="doc1" lang="en">
  <section id="s1" level="1" page="1">
    <title id="b1" role="title">Figures</title>
    <figure id="b2" page="1"><image src="{src}" alt="diagram"/><caption id="b3">A caption</caption></figure>
  </section>
</document>"#
    )
}

fn image_xobjects(pdf: &[u8]) -> Vec<(i64, i64)> {
    let doc = lopdf::Document::load_mem(pdf).expect("parse pdf");
    let mut dims = Vec::new();
    for (_, obj) in doc.objects.iter() {
        if let Object::Stream(s) = obj {
            let d = &s.dict;
            let is_image = d
                .get(b"Subtype")
                .ok()
                .and_then(|o| o.as_name().ok())
                == Some(b"Image");
            if is_image {
                let w = d.get(b"Width").unwrap().as_i64().unwrap();
                let h = d.get(b"Height").unwrap().as_i64().unwrap();
                dims.push((w, h));
            }
        }
    }
    dims
}

#[test]
fn figure_embeds_real_image() {
    let dir = std::env::temp_dir().join(format!("aipdf_fig_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let png = dir.join("diagram.png");
    image::RgbImage::from_pixel(6, 4, image::Rgb([200, 30, 30]))
        .save(&png)
        .unwrap();

    let pdf = build_aipdf(
        &doc_with_figure("diagram.png"),
        &BuildOptions {
            render: RenderMode::Full,
            base_dir: Some(dir.clone()),
            ..Default::default()
        },
    )
    .unwrap();

    let dims = image_xobjects(&pdf);
    assert_eq!(dims, vec![(6, 4)], "expected one 6x4 image xobject");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn missing_image_falls_back_to_placeholder() {
    let pdf = build_aipdf(
        &doc_with_figure("does-not-exist.png"),
        &BuildOptions {
            render: RenderMode::Full,
            base_dir: Some(std::env::temp_dir()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        image_xobjects(&pdf).is_empty(),
        "missing source must not embed an image"
    );
}
