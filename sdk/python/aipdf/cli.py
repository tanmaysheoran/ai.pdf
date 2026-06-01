"""Thin wrappers around the ``aipdf`` CLI for write-side and image operations.

The pure-Python read path (``AIPDF.open`` + ``to_*`` transforms) needs no
binary. But the write-side commands — ``build`` (PDF byte assembly, font
embedding, Brotli *compression*), ``ingest`` (lopdf parse + OCR), and image
extraction (PDF XObject decode + raster re-encode) — are owned by the Rust
core. Rather than re-implement them imperfectly in pure Python, these helpers
shell out to the installed ``aipdf`` binary, exactly as the MCP server already
does for ``ingest``.

Set ``AIPDF_BIN`` to override the binary name / path (default: ``aipdf``).

When the binary is not found on the first call, the SDK automatically
downloads a pre-built release binary for the current platform. Call
``install_binary_cli()`` (or run ``aipdf-install-cli``) to trigger this
explicitly without making a write-side call.
"""
from __future__ import annotations

import os
import platform
import subprocess
import sys
import sysconfig
import tarfile
import tempfile
import urllib.request
import zipfile
from dataclasses import dataclass, field
from pathlib import Path

from .core import AIPDFError

RENDER_MODES = ("minimal", "full", "browser")
PAGE_SIZES = ("letter", "a4")
OCR_MODES = ("auto", "never", "force")
EXPORT_FORMATS = ("xml", "markdown", "markdown-ast", "onto")

_REPO = "https://github.com/tanmaysheoran/ai.pdf"

# Maps (platform.system(), platform.machine()) → release artifact filename.
_ARTIFACTS: dict[tuple[str, str], str] = {
    ("Linux",   "x86_64"):  "aipdf-x86_64-linux.tar.gz",
    ("Linux",   "aarch64"): "aipdf-aarch64-linux.tar.gz",
    ("Darwin",  "x86_64"):  "aipdf-x86_64-macos.tar.gz",
    ("Darwin",  "arm64"):   "aipdf-aarch64-macos.tar.gz",
    ("Windows", "AMD64"):   "aipdf-x86_64-windows.zip",
}


def aipdf_binary() -> str:
    return os.environ.get("AIPDF_BIN", "aipdf")


def _install_dir() -> Path:
    """Return a writable directory on PATH where we can drop the binary."""
    scripts = Path(sysconfig.get_path("scripts"))
    if scripts.exists() and os.access(scripts, os.W_OK):
        return scripts
    local = Path.home() / ".local" / "bin"
    local.mkdir(parents=True, exist_ok=True)
    return local


def _install_binary() -> Path:
    """Download and install the pre-built aipdf binary for this platform.

    Returns the absolute path of the installed binary.
    Raises :class:`~aipdf.core.AIPDFError` if no pre-built binary exists for
    this platform or if the download/extraction fails.
    """
    key = (platform.system(), platform.machine())
    artifact = _ARTIFACTS.get(key)
    if artifact is None:
        raise AIPDFError(
            f"No pre-built aipdf binary for {key[0]}/{key[1]}. "
            f"Build from source: cargo install --git {_REPO} --bin aipdf aipdf-cli"
        )

    bin_name = "aipdf.exe" if platform.system() == "Windows" else "aipdf"
    dest_dir = _install_dir()
    bin_path = dest_dir / bin_name
    url = f"{_REPO}/releases/latest/download/{artifact}"

    print(f"aipdf: downloading {artifact}...", flush=True)
    try:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_archive = Path(tmp) / artifact
            urllib.request.urlretrieve(url, tmp_archive)

            if artifact.endswith(".tar.gz"):
                with tarfile.open(tmp_archive, "r:gz") as tf:
                    member = next((m for m in tf.getmembers() if m.name == "aipdf"), None)
                    if member is None:
                        raise AIPDFError("aipdf binary not found in release archive")
                    with tf.extractfile(member) as src:  # type: ignore[union-attr]
                        bin_path.write_bytes(src.read())
            else:
                with zipfile.ZipFile(tmp_archive) as zf:
                    entry = next(
                        (n for n in zf.namelist() if n == "aipdf.exe" or n == "aipdf"),
                        None,
                    )
                    if entry is None:
                        raise AIPDFError("aipdf binary not found in release zip")
                    bin_path.write_bytes(zf.read(entry))

            if platform.system() != "Windows":
                bin_path.chmod(0o755)

    except AIPDFError:
        raise
    except Exception as exc:
        raise AIPDFError(f"Failed to download/install aipdf: {exc}") from exc

    print(f"aipdf: installed → {bin_path}", flush=True)
    return bin_path


def install_binary_cli() -> None:
    """Console-script entry point for ``aipdf-install-cli``."""
    try:
        installed = _install_binary()
        print(f"aipdf installed → {installed}")
    except AIPDFError as exc:
        print(f"Error: {exc}", file=sys.stderr)
        sys.exit(1)


@dataclass
class ExportResult:
    """Files written by an ``export --save`` run."""

    output: Path
    images: list[Path] = field(default_factory=list)


def _run_cmd(cmd: list[str], *, timeout: int) -> str:
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired as exc:
        raise AIPDFError(
            f"aipdf {cmd[1] if len(cmd) > 1 else ''} timed out after {timeout}s"
        ) from exc
    if result.returncode != 0:
        raise AIPDFError(result.stderr.strip() or f"aipdf failed (exit {result.returncode})")
    return result.stdout


def _run(args: list[str], *, timeout: int = 180) -> str:
    cmd = [aipdf_binary(), *args]
    try:
        return _run_cmd(cmd, timeout=timeout)
    except FileNotFoundError as exc:
        # If AIPDF_BIN is set the user explicitly pointed at a path — don't override.
        if os.environ.get("AIPDF_BIN"):
            raise AIPDFError(
                f"aipdf CLI not found at {aipdf_binary()!r}; check AIPDF_BIN"
            ) from exc
        print("aipdf CLI not found — auto-installing...", flush=True)
        try:
            installed = _install_binary()
        except AIPDFError as install_err:
            raise AIPDFError(
                f"aipdf CLI not found and auto-install failed: {install_err}. "
                f"Build from source or run `aipdf-install-cli`."
            ) from exc
        return _run_cmd([str(installed), *args], timeout=timeout)


def build(
    input: str | Path,
    output: str | Path | None = None,
    *,
    render: str = "minimal",
    page_size: str = "letter",
    font: str | Path | None = None,
    title: str | None = None,
    timeout: int = 180,
) -> Path:
    """Build a ``.ai.pdf`` from a source file (.xml/.md/.html/.typ).

    ``render`` is one of ``minimal``/``full``/``browser`` (browser = headless
    Chrome, HTML input only). Returns the path the CLI wrote.
    """
    if render not in RENDER_MODES:
        raise AIPDFError(f"unknown render mode {render!r}; expected one of {RENDER_MODES}")
    if page_size not in PAGE_SIZES:
        raise AIPDFError(f"unknown page size {page_size!r}; expected one of {PAGE_SIZES}")
    args = ["build", str(input), "--render", render, "--page-size", page_size]
    if title is not None:
        args += ["--title", title]
    if font is not None:
        args += ["--font", str(font)]
    if output is not None:
        args += ["-o", str(output)]
    return Path(_run(args, timeout=timeout).strip())


def ingest(
    input: str | Path,
    output: str | Path | None = None,
    *,
    ocr: str = "auto",
    lang: str = "eng",
    timeout: int = 180,
) -> Path:
    """Attach a semantic layer to an existing PDF (text extraction + optional OCR)."""
    if ocr not in OCR_MODES:
        raise AIPDFError(f"unknown ocr mode {ocr!r}; expected one of {OCR_MODES}")
    args = ["ingest", str(input), "--ocr", ocr, "--lang", lang]
    if output is not None:
        args += ["-o", str(output)]
    return Path(_run(args, timeout=timeout).strip())


def export(
    file: str | Path,
    fmt: str = "xml",
    save: str | Path | None = None,
    *,
    timeout: int = 120,
) -> str | ExportResult:
    """Export the semantic layer via the CLI.

    Without ``save`` the rendered content is returned as a string. With
    ``save`` the content file and any extracted images are written to that
    directory and an :class:`ExportResult` (output path + image paths) is
    returned.
    """
    if fmt not in EXPORT_FORMATS:
        raise AIPDFError(f"unknown format {fmt!r}; expected one of {EXPORT_FORMATS}")
    args = ["export", str(file), "--format", fmt]
    if save is None:
        return _run(args, timeout=timeout)
    args += ["--save", str(save)]
    return _parse_saved(_run(args, timeout=timeout))


def extract_images(
    file: str | Path,
    out_dir: str | Path,
    *,
    fmt: str = "markdown",
    timeout: int = 120,
) -> list[Path]:
    """Extract embedded raster images to ``out_dir``, returning their paths.

    Runs ``export --save`` (which also writes the rendered content file) and
    returns only the image files. Use :func:`export` for the content file too.
    """
    result = export(file, fmt, out_dir, timeout=timeout)
    assert isinstance(result, ExportResult)
    return result.images


def bench(input: str | Path, *, timeout: int = 120) -> dict[str, str]:
    """Run ``aipdf bench`` and return its reported key: value pairs."""
    out = _run(["bench", str(input)], timeout=timeout)
    report: dict[str, str] = {}
    for line in out.splitlines():
        if ":" in line:
            key, _, value = line.partition(":")
            report[key.strip()] = value.strip()
    return report


def _parse_saved(stdout: str) -> ExportResult:
    # The CLI prints one `saved: <path>` line for the content file, then one
    # per extracted image. The first is the content; the rest are images.
    saved = [line[len("saved:"):].strip() for line in stdout.splitlines() if line.startswith("saved:")]
    if not saved:
        raise AIPDFError("export --save produced no output files")
    return ExportResult(output=Path(saved[0]), images=[Path(p) for p in saved[1:]])
