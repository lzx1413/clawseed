#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUNDLE_DIR="${1:?runtime build directory is required}"
ABI="${2:-arm64-v8a}"
DEST="$ROOT/clients/android/app/src/main/jniLibs/$ABI"

[[ -f "$BUNDLE_DIR/stage/proot" ]] || { printf 'missing staged proot\n' >&2; exit 1; }
[[ -f "$BUNDLE_DIR/stage/proot-loader" ]] || { printf 'missing staged proot-loader\n' >&2; exit 1; }
mkdir -p "$DEST"
cp "$BUNDLE_DIR/stage/proot" "$DEST/libproot.so"
cp "$BUNDLE_DIR/stage/proot-loader" "$DEST/libproot-loader.so"
chmod 0755 "$DEST/libproot.so" "$DEST/libproot-loader.so"
printf 'Packaged SELinux-safe PRoot bootstrap in %s\n' "$DEST"
