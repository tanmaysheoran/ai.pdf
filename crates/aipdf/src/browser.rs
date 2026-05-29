//! Browser-faithful visual layer for HTML inputs.
//!
//! Unlike `render.rs` (a hand-written layout engine that ignores CSS), this path
//! renders the original HTML — stylesheet and all — with a real browser engine
//! (headless Chrome / Chromium) so the visible PDF mirrors what the page looks
//! like in a browser: colours, backgrounds, borders, web fonts, table striping,
//! everything. The Brotli-compressed semantic XML is then attached to that PDF
//! via the same `lopdf` machinery `ingest` uses.
//!
//! We shell out to the Chrome CLI rather than take a heavy browser-automation
//! crate dependency — the same approach `ingest` takes with the `tesseract` CLI.
//! Chrome must be installed; if it is absent the call errors with an install
//! hint so the document can fall back to `--render full`/`minimal`.

use crate::render::PageOptions;
use crate::source::{semantic_xml_from_source, SourceKind};
use crate::{ingest, AipdfError, BuildOptions, Result};
use lopdf::Document;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Hard ceiling on how long we let Chrome run before giving up. The PDF for a
/// self-contained page is normally written in a second or two; this only fires
/// for genuinely stuck renders.
const RENDER_TIMEOUT: Duration = Duration::from_secs(30);

/// Build a `.ai.pdf` from raw HTML: render the *visible* layer with a browser
/// (full CSS fidelity), derive the *machine* layer (semantic XML) from the same
/// HTML, then attach the latter to the former.
///
/// `base_dir` is the directory the HTML lives in; relative resource URLs
/// (`<img src="1.jpg">`, stylesheets, fonts) are resolved against it, so it must
/// point at the real asset directory for images to appear.
pub fn build_aipdf_browser(
    html: &str,
    base_dir: Option<&Path>,
    options: &BuildOptions,
) -> Result<Vec<u8>> {
    // Machine layer: semantic XML (sanitised + validated inside).
    let xml = semantic_xml_from_source(html, SourceKind::Html)?;

    // Visible layer: a browser-rendered PDF of the original HTML.
    let pdf = render_html_to_pdf(html, base_dir, &options.page)?;

    // Attach the semantic layer to the browser's PDF.
    let mut doc = Document::load_mem(&pdf)
        .map_err(|e| AipdfError::Pdf(format!("cannot parse browser-rendered PDF: {e}")))?;
    ingest::attach_semantic_layer(&mut doc, &xml, "html-browser")?;

    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| AipdfError::Pdf(format!("cannot serialize PDF: {e}")))?;
    Ok(out)
}

/// Whether a usable Chrome/Chromium binary can be located.
pub fn chrome_available() -> bool {
    find_chrome().is_some()
}

/// Render HTML to a PDF with headless Chrome, honouring the page size and
/// printing CSS backgrounds. Returns the raw PDF bytes.
fn render_html_to_pdf(html: &str, base_dir: Option<&Path>, page: &PageOptions) -> Result<Vec<u8>> {
    let chrome = find_chrome().ok_or_else(|| {
        AipdfError::Pdf(
            "browser render requires Google Chrome or Chromium, which was not found \
             (install it, or set AIPDF_CHROME to its path; or use `--render full`)"
                .into(),
        )
    })?;

    // We inject a small print stylesheet (exact page size + force background
    // printing) and write the result next to the original assets so relative
    // URLs still resolve, then point Chrome at that file.
    let dir: PathBuf = match base_dir {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let injected = inject_print_styles(html, page);
    let stamp = std::process::id();
    let html_tmp = dir.join(format!(".aipdf-print-{stamp}.html"));
    std::fs::write(&html_tmp, injected.as_bytes())?;

    let out_pdf = std::env::temp_dir().join(format!("aipdf-browser-{stamp}.pdf"));
    let user_data = std::env::temp_dir().join(format!("aipdf-chrome-{stamp}"));
    let file_url = file_url(&html_tmp);
    // Stale output from a prior run with the same pid would fool the poller.
    let _ = std::fs::remove_file(&out_pdf);

    // Spawn Chrome with `spawn()` (not `status()`/`output()`) and never wait on
    // it: many Chrome builds finish writing the PDF but then *fail to exit* in
    // `--headless --print-to-pdf` mode, so waiting on the process hangs for a
    // minute or more even though the PDF is already on disk. Instead we poll for
    // the output file to be fully written, then kill Chrome ourselves. Null
    // stdio also keeps helper processes from holding inherited pipes open.
    //
    // The extra flags suppress first-run wizards, component/safebrowsing updates
    // and other background networking that otherwise add startup latency, and
    // `--virtual-time-budget` lets any page timers settle deterministically.
    let spawn = Command::new(&chrome)
        .arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-sandbox")
        .arg("--no-pdf-header-footer")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-extensions")
        .arg("--disable-background-networking")
        .arg("--disable-component-update")
        .arg("--disable-sync")
        .arg("--disable-default-apps")
        .arg("--metrics-recording-only")
        .arg("--mute-audio")
        .arg("--virtual-time-budget=5000")
        .arg(format!("--user-data-dir={}", user_data.display()))
        .arg(format!("--print-to-pdf={}", out_pdf.display()))
        .arg(&file_url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    let mut child = match spawn {
        Ok(c) => c,
        Err(e) => {
            let _ = std::fs::remove_file(&html_tmp);
            let _ = std::fs::remove_dir_all(&user_data);
            return Err(AipdfError::Pdf(format!(
                "failed to launch Chrome ({}): {e}",
                chrome.display()
            )));
        }
    };

    // Poll until the PDF is completely written (or Chrome exits / we time out).
    // "Complete" = the file has a %PDF- header, a %%EOF trailer, and a size that
    // stopped changing between two polls — so we never read a half-flushed file.
    let deadline = Instant::now() + RENDER_TIMEOUT;
    let mut last_len = 0u64;
    let ready = loop {
        let exited = matches!(child.try_wait(), Ok(Some(_)));
        let len = std::fs::metadata(&out_pdf).map(|m| m.len()).unwrap_or(0);
        if len > 0 && len == last_len && pdf_is_complete(&out_pdf) {
            break true;
        }
        last_len = len;
        if exited {
            // Chrome quit on its own — give it one final verdict.
            break pdf_is_complete(&out_pdf);
        }
        if Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(Duration::from_millis(150));
    };

    // Stop Chrome (it may still be lingering) and clean up temp artefacts. The
    // injected HTML must not be left behind in the user's asset directory.
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&html_tmp);
    let _ = std::fs::remove_dir_all(&user_data);

    let read = std::fs::read(&out_pdf);
    let _ = std::fs::remove_file(&out_pdf);

    match read {
        Ok(bytes) if ready && bytes.starts_with(b"%PDF-") => Ok(bytes),
        _ => Err(AipdfError::Pdf(format!(
            "Chrome ({}) did not produce a PDF within {}s",
            chrome.display(),
            RENDER_TIMEOUT.as_secs()
        ))),
    }
}

/// Whether the file at `path` looks like a fully-written PDF: a `%PDF-` header
/// and a `%%EOF` trailer within the final kilobyte (Chrome may append a newline
/// or a little whitespace after the marker).
fn pdf_is_complete(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    if !bytes.starts_with(b"%PDF-") {
        return false;
    }
    let tail = &bytes[bytes.len().saturating_sub(1024)..];
    tail.windows(5).any(|w| w == b"%%EOF")
}

/// Inject a print stylesheet that pins the paper size to `PageOptions` and forces
/// CSS backgrounds to print. Inserted just before `</head>` (or `<body>`, or the
/// document start) so it wins the cascade for `@page`/`print-color-adjust`.
fn inject_print_styles(html: &str, page: &PageOptions) -> String {
    let style = format!(
        "<style>@page{{size:{w:.0}pt {h:.0}pt;}}\
         html{{-webkit-print-color-adjust:exact;print-color-adjust:exact;}}</style>",
        w = page.width,
        h = page.height,
    );
    if let Some(idx) = html.find("</head>") {
        let mut s = String::with_capacity(html.len() + style.len());
        s.push_str(&html[..idx]);
        s.push_str(&style);
        s.push_str(&html[idx..]);
        s
    } else if let Some(idx) = html.find("<body") {
        let mut s = String::with_capacity(html.len() + style.len());
        s.push_str(&html[..idx]);
        s.push_str(&style);
        s.push_str(&html[idx..]);
        s
    } else {
        format!("{style}{html}")
    }
}

/// Build a `file://` URL for an absolute (or canonicalisable) path, percent-
/// encoding spaces so Chrome accepts it.
fn file_url(path: &Path) -> String {
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let s = abs.to_string_lossy().replace(' ', "%20");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        // Windows paths (C:\...) become file:///C:/...
        format!("file:///{}", s.replace('\\', "/"))
    }
}

/// Locate a Chrome/Chromium executable: `AIPDF_CHROME` first, then well-known
/// install locations per platform, then common PATH names.
fn find_chrome() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AIPDF_CHROME") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }

    let fixed: &[&str] = &[
        // macOS
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
        // Linux
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/snap/bin/chromium",
        // Windows
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    ];
    for cand in fixed {
        let p = PathBuf::from(cand);
        if p.is_file() {
            return Some(p);
        }
    }

    // PATH lookup by name.
    let names = [
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "chrome",
    ];
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in names {
            let cand = dir.join(name);
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}
