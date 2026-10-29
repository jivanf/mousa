#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --locked --example workbench
# Replace atomically so an existing preview process can exit normally.
install -Dm755 target/release/examples/workbench "$HOME/.local/bin/mousa-workbench.new"
mv -f "$HOME/.local/bin/mousa-workbench.new" "$HOME/.local/bin/mousa-workbench"
cmp target/release/examples/workbench "$HOME/.local/bin/mousa-workbench"
install -Dm644 assets/mousa-workbench.desktop "$HOME/.local/share/applications/mousa-workbench.desktop"
install -Dm644 assets/mousa.svg "$HOME/.local/share/icons/hicolor/scalable/apps/mousa.svg"
if command -v update-desktop-database >/dev/null; then
  update-desktop-database "$HOME/.local/share/applications"
fi
printf 'Installed Mousa Workbench. Launch it from your applications menu.\n'
