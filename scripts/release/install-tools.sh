#!/usr/bin/env bash
set -euo pipefail
# Linux CI only; application builds and policy tests also run on Windows.
tool_dir="${RUNNER_TEMP:-/tmp}/mousa-release-tools"
mkdir -p "$tool_dir"
curl --fail --location --retry 3 --connect-timeout 30 --max-time 600 \
  https://github.com/release-plz/release-plz/releases/download/release-plz-v0.4.1/release-plz-x86_64-unknown-linux-gnu.tar.gz \
  -o "$tool_dir/release-plz.tar.gz"
printf '%s  %s\n' 4b70642e4270eeb7c073a2f16c4c168f9558f4e32bb98f0dd751b26eacd8bef1 "$tool_dir/release-plz.tar.gz" | sha256sum -c -
tar -xzf "$tool_dir/release-plz.tar.gz" -C "$tool_dir"
echo "$tool_dir" >> "$GITHUB_PATH"
