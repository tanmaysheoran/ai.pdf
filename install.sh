#!/usr/bin/env bash
# install.sh — install the aipdf CLI
#
# Usage (remote):
#   curl -fsSL https://raw.githubusercontent.com/tanmaysheoran/ai.pdf/main/install.sh | sh
#
# Usage (local repo):
#   ./install.sh
#
# Overrides:
#   AIPDF_REPO=https://github.com/your-fork/ai.pdf ./install.sh
#   AIPDF_BUILD=1 ./install.sh    # force a source build even when binaries exist

set -euo pipefail

REPO="${AIPDF_REPO:-https://github.com/tanmaysheoran/ai.pdf}"

say()  { printf '\033[1m%s\033[0m\n' "$*"; }
ok()   { printf '\033[32m✓\033[0m %s\n' "$*"; }
warn() { printf '\033[33mNote:\033[0m %s\n' "$*"; }
err()  { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }

# ── platform detection ────────────────────────────────────────────────────────

detect_artifact() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"
    case "${os}-${arch}" in
        Linux-x86_64)                echo "aipdf-x86_64-linux.tar.gz" ;;
        Linux-aarch64)               echo "aipdf-aarch64-linux.tar.gz" ;;
        Darwin-x86_64)               echo "aipdf-x86_64-macos.tar.gz" ;;
        Darwin-arm64|Darwin-aarch64) echo "aipdf-aarch64-macos.tar.gz" ;;
        *) return 1 ;;
    esac
}

# ── install directory ─────────────────────────────────────────────────────────

install_dir() {
    local cargo_bin="${CARGO_HOME:-$HOME/.cargo}/bin"
    if mkdir -p "$cargo_bin" 2>/dev/null; then
        echo "$cargo_bin"
    elif [ -w /usr/local/bin ]; then
        echo /usr/local/bin
    else
        mkdir -p "$HOME/.local/bin"
        echo "$HOME/.local/bin"
    fi
}

# ── download pre-built binary ─────────────────────────────────────────────────

try_download() {
    local artifact="$1" dir="$2"
    local url="${REPO}/releases/latest/download/${artifact}"
    local tmp
    tmp="$(mktemp -d)"

    say "Downloading pre-built binary..."
    if curl -fsSL "$url" -o "$tmp/$artifact" 2>/dev/null; then
        tar xzf "$tmp/$artifact" -C "$tmp"
        if [ -f "$tmp/aipdf" ]; then
            chmod +x "$tmp/aipdf"
            mv "$tmp/aipdf" "$dir/aipdf"
            rm -rf "$tmp"
            return 0
        fi
    fi

    rm -rf "$tmp"
    return 1
}

# ── source build (cargo) ──────────────────────────────────────────────────────

ensure_cargo() {
    if command -v cargo &>/dev/null; then return; fi
    say "Rust not found — installing via rustup..."
    command -v curl &>/dev/null || err "curl is required. Install it and re-run."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
    CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
    # shellcheck source=/dev/null
    . "$CARGO_HOME/env"
}

install_from_source() {
    ensure_cargo
    if [ -f "crates/aipdf-cli/Cargo.toml" ]; then
        say "Building aipdf from local source..."
        cargo install --path crates/aipdf-cli
    elif [ -n "${AIPDF_SOURCE:-}" ] && [ -f "$AIPDF_SOURCE/crates/aipdf-cli/Cargo.toml" ]; then
        say "Building aipdf from $AIPDF_SOURCE..."
        cargo install --path "$AIPDF_SOURCE/crates/aipdf-cli"
    else
        say "Building aipdf from source (this takes a minute)..."
        cargo install --git "$REPO" --bin aipdf aipdf-cli
    fi
}

# ── main ──────────────────────────────────────────────────────────────────────

main() {
    # Always build from source when: inside the repo, AIPDF_BUILD is set, or AIPDF_SOURCE is set.
    if [ -n "${AIPDF_BUILD:-}" ] || [ -n "${AIPDF_SOURCE:-}" ] || [ -f "crates/aipdf-cli/Cargo.toml" ]; then
        install_from_source
    else
        local artifact dir
        dir="$(install_dir)"
        if ! artifact="$(detect_artifact 2>/dev/null)"; then
            warn "No pre-built binary for this platform — building from source (requires Rust)."
            install_from_source
        elif ! try_download "$artifact" "$dir"; then
            warn "Download failed — building from source (requires Rust)."
            install_from_source
        fi
    fi

    local bin_path
    bin_path="$(command -v aipdf 2>/dev/null || echo "${CARGO_HOME:-$HOME/.cargo}/bin/aipdf")"

    echo ""
    ok "aipdf installed → $bin_path"
    echo ""

    local bin_dir
    bin_dir="$(dirname "$bin_path")"
    case ":${PATH}:" in
        *":$bin_dir:"*) ;;
        *)
            warn "$bin_dir is not on your PATH."
            printf 'Add this to your shell profile (~/.bashrc, ~/.zshrc, …):\n\n'
            printf '  export PATH="%s:$PATH"\n\n' "$bin_dir"
            ;;
    esac

    echo "Get started:"
    echo "  aipdf --help"
    echo "  aipdf build samples/minimal.xml -o out.ai.pdf"
    echo "  aipdf inspect out.ai.pdf"
}

main "$@"
