use aipdf::{
    build_aipdf, build_aipdf_browser, extract_images, extract_semantic_xml, ingest_pdf,
    inspect_pdf, semantic_xml_from_source, validate_xml, xml_to_markdown, xml_to_markdown_ast_json,
    xml_to_onto, BuildOptions, Font, IngestOptions, OcrMode, PageOptions, RenderMode, SourceKind,
};
use clap::{Parser, Subcommand, ValueEnum};
use std::{fs, path::PathBuf};

#[derive(Parser)]
#[command(name = "aipdf")]
#[command(about = "Build and inspect semantic PDF extension files")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Build {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long, default_value = "AIPDF Document")]
        title: String,
        #[arg(long, value_enum, default_value_t = Render::Minimal)]
        render: Render,
        #[arg(long, value_enum, default_value_t = PageSize::Letter)]
        page_size: PageSize,
        /// Path to a TrueType font to embed in the visible layer (e.g. a Noto
        /// CJK face). Defaults to the bundled DejaVu Sans.
        #[arg(long)]
        font: Option<PathBuf>,
    },
    /// Attach a semantic layer to an existing PDF (text extraction + optional OCR).
    Ingest {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = Ocr::Auto)]
        ocr: Ocr,
        #[arg(long, default_value = "eng")]
        lang: String,
    },
    Inspect {
        file: PathBuf,
    },
    Extract {
        file: PathBuf,
    },
    Validate {
        file: PathBuf,
    },
    Export {
        file: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Xml)]
        format: Format,
        /// Save output and extracted images to this directory instead of printing to stdout.
        #[arg(long)]
        save: Option<PathBuf>,
    },
    Bench {
        input: PathBuf,
    },
}

#[derive(Clone, ValueEnum)]
enum Format {
    Xml,
    Markdown,
    MarkdownAst,
    Onto,
}

#[derive(Clone, ValueEnum, Default)]
enum Render {
    #[default]
    Minimal,
    Full,
    /// Browser-faithful CSS rendering via headless Chrome (HTML input only).
    Browser,
}

#[derive(Clone, ValueEnum, Default)]
enum PageSize {
    #[default]
    Letter,
    A4,
}

#[derive(Clone, ValueEnum, Default)]
enum Ocr {
    #[default]
    Auto,
    Never,
    Force,
}

fn main() -> aipdf::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Build {
            input,
            output,
            title,
            render,
            page_size,
            font,
        } => {
            let source = fs::read_to_string(&input)?;
            let kind = SourceKind::from_path(&input)?;
            let page = match page_size {
                PageSize::Letter => PageOptions::letter(),
                PageSize::A4 => PageOptions::a4(),
            };
            let font = match font {
                Some(path) => Font::from_path(&path)?,
                None => Font::default_font(),
            };
            // Resolve relative figure image paths against the input's directory.
            let base_dir = input.parent().map(|p| p.to_path_buf());
            let bytes = if matches!(render, Render::Browser) {
                // Browser render reads the original markup (CSS and all), not the
                // lowered semantic XML, so it only applies to HTML input.
                if kind != SourceKind::Html {
                    return Err(aipdf::AipdfError::InvalidXml(
                        "`--render browser` requires HTML input; use `--render full` for other formats".into(),
                    ));
                }
                build_aipdf_browser(
                    &source,
                    base_dir.as_deref(),
                    &BuildOptions {
                        title,
                        visible_text: None,
                        render: RenderMode::Full,
                        page,
                        font,
                        base_dir: base_dir.clone(),
                    },
                )?
            } else {
                let xml = semantic_xml_from_source(&source, kind)?;
                let render_mode = match render {
                    Render::Minimal => RenderMode::Minimal,
                    Render::Full => RenderMode::Full,
                    Render::Browser => unreachable!(),
                };
                build_aipdf(
                    &xml,
                    &BuildOptions {
                        title,
                        visible_text: None,
                        render: render_mode,
                        page,
                        font,
                        base_dir,
                    },
                )?
            };
            let output = output.unwrap_or_else(|| {
                let stem = input.file_stem().unwrap_or(input.as_os_str());
                input.with_file_name(format!("{}.ai.pdf", stem.to_string_lossy()))
            });
            fs::write(&output, bytes)?;
            println!("{}", output.display());
        }
        Command::Ingest {
            input,
            output,
            ocr,
            lang,
        } => {
            let bytes = fs::read(&input)?;
            let ocr = match ocr {
                Ocr::Auto => OcrMode::Auto,
                Ocr::Never => OcrMode::Never,
                Ocr::Force => OcrMode::Force,
            };
            let out_bytes = ingest_pdf(&bytes, &IngestOptions { ocr, lang })?;
            let output = output.unwrap_or_else(|| {
                let stem = input.file_stem().unwrap_or(input.as_os_str());
                input.with_file_name(format!("{}.ai.pdf", stem.to_string_lossy()))
            });
            fs::write(&output, out_bytes)?;
            println!("{}", output.display());
        }
        Command::Inspect { file } => {
            let bytes = fs::read(&file)?;
            let report = inspect_pdf(&bytes);
            println!("is_pdf: {}", report.is_pdf);
            println!("has_semantic_layer: {}", report.has_semantic_layer);
            if let Some(n) = report.semantic_compressed_bytes {
                println!("semantic_compressed_bytes: {n}");
            }
            if let Some(n) = report.semantic_xml_bytes {
                println!("semantic_xml_bytes: {n}");
            }
        }
        Command::Extract { file } => {
            let bytes = fs::read(&file)?;
            println!("{}", extract_semantic_xml(&bytes)?);
        }
        Command::Validate { file } => {
            let bytes = fs::read(&file)?;
            let xml = extract_semantic_xml(&bytes)?;
            validate_xml(&xml)?;
            println!("valid");
        }
        Command::Export { file, format, save } => {
            let bytes = fs::read(&file)?;
            let xml = extract_semantic_xml(&bytes)?;
            let content = match format {
                Format::Xml => xml.clone(),
                Format::Markdown => xml_to_markdown(&xml),
                Format::MarkdownAst => xml_to_markdown_ast_json(&xml),
                Format::Onto => xml_to_onto(&xml),
            };
            match save {
                None => println!("{content}"),
                Some(dir) => {
                    fs::create_dir_all(&dir)?;
                    let stem = file
                        .file_stem()
                        .unwrap_or(file.as_os_str())
                        .to_string_lossy();
                    // Strip a double extension like "doc.ai" → "doc" for "doc.ai.pdf".
                    let stem = stem.trim_end_matches(".ai");
                    let ext = match format {
                        Format::Xml => "xml",
                        Format::Markdown => "md",
                        Format::MarkdownAst => "json",
                        Format::Onto => "onto",
                    };
                    let out_path = dir.join(format!("{stem}.{ext}"));
                    fs::write(&out_path, &content)?;
                    println!("saved: {}", out_path.display());

                    let imgs = extract_images(&bytes)?;
                    if imgs.is_empty() {
                        // Check whether the XML actually had image refs so we
                        // can give a useful hint rather than silent omission.
                        let xml2 = extract_semantic_xml(&bytes)?;
                        if xml2.contains("<image ") {
                            eprintln!("note: document contains image references but no extractable image XObjects were found");
                            eprintln!("      image extraction requires a PDF built with --render full");
                        }
                    }
                    for img in imgs {
                        let saved = img.save_to(&dir)?;
                        println!("saved: {}", saved.display());
                    }
                }
            }
        }
        Command::Bench { input } => {
            let xml = fs::read_to_string(&input)?;
            let pdf = build_aipdf(&xml, &BuildOptions::default())?;
            println!("xml_bytes: {}", xml.len());
            println!("aipdf_bytes: {}", pdf.len());
            println!("semantic_layer: embedded as Brotli XML associated file");
        }
    }
    Ok(())
}
