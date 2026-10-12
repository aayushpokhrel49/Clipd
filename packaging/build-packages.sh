#!/usr/bin/env bash
# Build native .deb / .rpm / Arch (.pkg.tar.zst) / Alpine (.apk) packages for the
# host architecture using nfpm (https://nfpm.goreleaser.com).
#
#   VERSION=0.2.0 ./packaging/build-packages.sh
#
# Override the architecture with ARCH=amd64|arm64 (defaults to the host arch).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

version="${VERSION:-$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)}"
version="${version#v}"

case "$(uname -m)" in
  x86_64 | amd64) host_arch=amd64 ;;
  aarch64 | arm64) host_arch=arm64 ;;
  *)
    echo "unsupported host architecture: $(uname -m)" >&2
    exit 1
    ;;
esac
arch="${ARCH:-$host_arch}"

if ! command -v nfpm >/dev/null 2>&1; then
  echo "!! nfpm was not found." >&2
  echo "   Install it from https://nfpm.goreleaser.com/install/ and re-run." >&2
  exit 1
fi

echo ">> Building clipd $version for $arch"
cargo build --release --locked --offline 2>/dev/null || cargo build --release --locked

mkdir -p dist
install -m 0755 target/release/clipd dist/clipd
strip dist/clipd 2>/dev/null || true

for fmt in deb rpm archlinux apk; do
  echo ">> Packaging $fmt"
  VERSION="$version" ARCH="$arch" nfpm package \
    -f packaging/nfpm.yaml -p "$fmt" -t dist/
done

echo ">> Done. Packages in $root/dist:"
ls -1 dist/*.deb dist/*.rpm dist/*.pkg.tar.zst dist/*.apk 2>/dev/null || true
