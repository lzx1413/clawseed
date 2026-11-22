#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUNDLE_DIR="${1:-$ROOT/build/proot-runtime-arm64}"
ABI="${2:-arm64-v8a}"
DEST="$BUNDLE_DIR/packaged/jniLibs/$ABI"
ASSETS="$BUNDLE_DIR/packaged/assets/clawseed-runtime"
mkdir -p "$DEST" "$ASSETS"
cp "$BUNDLE_DIR/stage/proot" "$DEST/libproot.so"
cp "$BUNDLE_DIR/stage/proot-loader" "$DEST/libproot-loader.so"
cp "$BUNDLE_DIR/stage/proot-libs/libtalloc.so.2" "$DEST/libtalloc.so"
cp "$BUNDLE_DIR/stage/proot-libs/libandroid-shmem.so" "$DEST/libandroid-shmem.so"
# Android packages lib*.so names only. Preserve ELF offsets while rewriting the
# dependency and SONAME; runtime copies in the tar retain their original names.
python3 - "$DEST" <<'PY'
from pathlib import Path
import sys
for name in ('libproot.so', 'libtalloc.so'):
    path = Path(sys.argv[1]) / name
    old = b'libtalloc.so.2\0'
    new = b'libtalloc.so\0'.ljust(len(old), b'\0')
    data = path.read_bytes()
    if old not in data:
        raise SystemExit(f'Missing expected ELF dependency in {path}')
    path.write_bytes(data.replace(old, new))
PY
chmod 0755 "$DEST"/*.so
cp "$BUNDLE_DIR/manifest.json" "$ASSETS/manifest.json"
cp "$BUNDLE_DIR/clawseed-proot-runtime-$ABI.tar.xz" "$ASSETS/runtime.tar.xz"
printf 'APK runtime inputs: %s/packaged\n' "$BUNDLE_DIR"
