#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
bin_dir="${XDG_BIN_HOME:-${HOME}/.local/bin}"
config_dir="${XDG_CONFIG_HOME:-${HOME}/.config}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "!! cargo was not found." >&2
  echo "   Install the Rust toolchain (https://rustup.rs) and the GTK3 dev headers:" >&2
  echo "     Debian/Ubuntu : sudo apt install build-essential pkg-config libgtk-3-dev" >&2
  echo "     Fedora        : sudo dnf install gcc pkg-config gtk3-devel" >&2
  echo "     Arch          : sudo pacman -S base-devel pkgconf gtk3" >&2
  echo "     openSUSE      : sudo zypper install gcc pkg-config gtk3-devel" >&2
  echo "     Alpine        : sudo apk add build-base pkgconf gtk+3.0-dev" >&2
  exit 1
fi

echo ">> Building clipd (release)..."
( cd "$here" && cargo build --release --offline || cargo build --release )

mkdir -p "$bin_dir"
install -Dm755 "$here/target/release/clipd" "$bin_dir/clipd"
echo ">> Installed binary -> $bin_dir/clipd"

autostart="${config_dir}/autostart"
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
echo "clipd will autostart on next login (XDG autostart, honoured by GNOME, KDE,"
echo "XFCE, Cinnamon, MATE, LXQt, ...). It grabs Ctrl+Alt+V itself, so no desktop"
echo "keybinding needs to be configured."
if ! command -v curl >/dev/null 2>&1; then
  echo
  echo "note: 'curl' was not found — automatic updates will be skipped."
fi
