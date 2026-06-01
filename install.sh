#!/usr/bin/env bash
# install.sh — install the aipdf CLI
#
# Usage (from anywhere):
#   curl -fsSL https://raw.githubusercontent.com/tanmaysheoran/ai.pdf/main/install.sh | sh
#
# Usage (from inside the cloned repo):
#   ./install.sh
#
# Override the source repo:
#   AIPDF_REPO=https://github.com/your-fork/aipdf ./install.sh

set -euo pipefail

REPO="${AIPDF_REPO:-https://github.com/tanmaysheoran/ai.pdf}"

# ── helpers ──────────────────────────────────────────────────────────────────

say()  { printf '\033[1m%s\033[0m\n' "$*"; }
ok()   { printf '\033[32m✓\033[0m %s\n' "$*"; }
err()  { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }

# ── ensure Rust / cargo ───────────────────────────────────────────────────────

ensure_cargo() {
    if command -v cargo &>/dev/null; then
        return
    fi

    say "Rust not found — installing via rustup..."
    if ! command -v curl &>/dev/null; then
        err "curl is required to install rustup. Install it and re-run."
    fi

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path

    # Make cargo available for the remainder of this script.
    CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
    # shellcheck source=/dev/null
    . "$CARGO_HOME/env"
}

# ── install ───────────────────────────────────────────────────────────────────

main() {
    ensure_cargo

    # Prefer a local source tree so contributors can dogfood their own builds.
    if [ -f "crates/aipdf-cli/Cargo.toml" ]; then
        say "Installing aipdf from local source..."
        cargo install --path crates/aipdf-cli
    elif [ -n "${AIPDF_SOURCE:-}" ] && [ -f "$AIPDF_SOURCE/crates/aipdf-cli/Cargo.toml" ]; then
        say "Installing aipdf from $AIPDF_SOURCE..."
        cargo install --path "$AIPDF_SOURCE/crates/aipdf-cli"
    else
        say "Installing aipdf from $REPO ..."
        cargo install --git "$REPO" --bin aipdf aipdf-cli
    fi

    # Friendly post-install message.
    CARGO_BIN="${CARGO_HOME:-$HOME/.cargo}/bin"
    if command -v aipdf &>/dev/null; then
        BIN_PATH="$(command -v aipdf)"
    else
        BIN_PATH="$CARGO_BIN/aipdf"
    fi

    echo ""
    ok "aipdf installed → $BIN_PATH"
    echo ""

    # Warn if ~/.cargo/bin is not yet on PATH.
    case ":${PATH}:" in
        *":$CARGO_BIN:"*) ;;
        *)
            printf '\033[33mNote:\033[0m %s is not on your PATH.\n' "$CARGO_BIN"
            echo "Add this to your shell profile (~/.bashrc, ~/.zshrc, …):"
            echo ""
            echo "  export PATH=\"\$HOME/.cargo/bin:\$PATH\""
            echo ""
            ;;
    esac

    echo "Get started:"
    echo "  aipdf --help"
    echo "  aipdf build  samples/minimal.xml -o out.ai.pdf"
    echo "  aipdf inspect out.ai.pdf"
}

main "$@"
