#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --locked
install -Dm755 target/release/mousa "$HOME/.local/bin/mousa"
install -Dm755 target/release/mousa-launcher "$HOME/.local/bin/mousa-launcher"
"$HOME/.local/bin/mousa-launcher" --register
install -Dm644 assets/mousa.desktop "$HOME/.local/share/applications/mousa.desktop"
install -Dm644 assets/mousa.svg "$HOME/.local/share/icons/hicolor/scalable/apps/mousa.svg"
if command -v update-desktop-database >/dev/null; then
  update-desktop-database "$HOME/.local/share/applications"
fi
printf 'Installed Mousa. Launch it from your applications menu.\n'
