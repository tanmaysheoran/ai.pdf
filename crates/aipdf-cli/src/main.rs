use aipdf::{
    build_aipdf, extract_semantic_xml, inspect_pdf, semantic_xml_from_source, validate_xml,
    xml_to_markdown, xml_to_markdown_ast_json, xml_to_onto, BuildOptions, PageOptions,
    RenderMode, SourceKind,
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
}

#[derive(Clone, ValueEnum, Default)]
enum PageSize {
    #[default]
    Letter,
    A4,
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
        } => {
            let source = fs::read_to_string(&input)?;
            let kind = SourceKind::from_path(&input)?;
            let xml = semantic_xml_from_source(&source, kind)?;
            let render_mode = match render {
                Render::Minimal => RenderMode::Minimal,
                Render::Full => RenderMode::Full,
            };
            let page = match page_size {
                PageSize::Letter => PageOptions::letter(),
                PageSize::A4 => PageOptions::a4(),
            };
            let bytes = build_aipdf(
                &xml,
                &BuildOptions {
                    title,
                    visible_text: None,
                    render: render_mode,
                    page,
                },
            )?;
            let output = output.unwrap_or_else(|| {
                let stem = input.file_stem().unwrap_or(input.as_os_str());
                input.with_file_name(format!("{}.ai.pdf", stem.to_string_lossy()))
            });
            fs::write(&output, bytes)?;
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
        Command::Export { file, format } => {
            let bytes = fs::read(&file)?;
            let xml = extract_semantic_xml(&bytes)?;
            match format {
                Format::Xml => println!("{xml}"),
                Format::Markdown => println!("{}", xml_to_markdown(&xml)),
                Format::MarkdownAst => println!("{}", xml_to_markdown_ast_json(&xml)),
                Format::Onto => println!("{}", xml_to_onto(&xml)),
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
