#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "==> Building ward (release)..."
cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"

BIN="$SCRIPT_DIR/target/release/ward"
DEST="$HOME/.cargo/bin/ward"

echo "==> Installing binary to $DEST"
cp "$BIN" "$DEST"
chmod +x "$DEST"

# ── Linux: systemd user service ──────────────────────────────────────────────
if [[ "$(uname)" == "Linux" ]]; then
    SERVICE_DIR="$HOME/.config/systemd/user"
    mkdir -p "$SERVICE_DIR"
    cp "$SCRIPT_DIR/assets/ward.service" "$SERVICE_DIR/ward.service"
    systemctl --user daemon-reload
    systemctl --user enable --now ward
    echo "==> systemd user service enabled (ward daemon)"

# ── macOS: launchd agent ──────────────────────────────────────────────────────
elif [[ "$(uname)" == "Darwin" ]]; then
    PLIST_SRC="$SCRIPT_DIR/assets/ward.plist"
    PLIST_DEST="$HOME/Library/LaunchAgents/com.ward.daemon.plist"
    # Patch the binary path into the plist
    sed "s|/Users/USER|$HOME|g" "$PLIST_SRC" > "$PLIST_DEST"
    launchctl unload "$PLIST_DEST" 2>/dev/null || true
    launchctl load -w "$PLIST_DEST"
    echo "==> launchd agent loaded (ward daemon)"
fi

# ── Fish completions ──────────────────────────────────────────────────────────
FISH_COMP_DIR="$HOME/.config/fish/completions"
if [[ -d "$FISH_COMP_DIR" ]]; then
    cp "$SCRIPT_DIR/assets/completions/ward.fish" "$FISH_COMP_DIR/ward.fish"
    echo "==> Fish completions installed"
fi

echo ""
echo "Done! Run: ward"
