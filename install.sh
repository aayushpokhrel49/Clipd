#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
bin_dir="${HOME}/.local/bin"

echo ">> Building clipd (release)..."
( cd "$here" && cargo build --release --offline || cargo build --release )

mkdir -p "$bin_dir"
install -Dm755 "$here/target/release/clipd" "$bin_dir/clipd"
echo ">> Installed binary -> $bin_dir/clipd"

autostart="${HOME}/.config/autostart"
mkdir -p "$autostart"
sed "s|@CLIPD@|${bin_dir}/clipd|g" "$here/clipd.desktop" > "$autostart/clipd.desktop"
echo ">> Installed autostart -> $autostart/clipd.desktop"

echo
echo "Done."
echo "Start now      : ${bin_dir}/clipd start &"
echo "Toggle popup   : ${bin_dir}/clipd toggle   (or press Ctrl+Alt+V)"
echo "Clear history  : ${bin_dir}/clipd clear"
echo "Stop daemon    : ${bin_dir}/clipd quit"
echo
echo "clipd will autostart on next login. It grabs Ctrl+Alt+V itself,"
echo "so no desktop keybinding needs to be configured."
