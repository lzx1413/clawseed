#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# Reuse the gateway build's Android NDK, compiler and bindgen environment.
source "$ROOT/tools/build-clawseed-android.sh" aarch64 build
cargo build -p clawseed-tools --locked --release --target "$TARGET" \
  --features android --example command_backends_probe
PROBE_DEST="$ROOT/clients/android/app/src/androidTest/jniLibs/$ABI"
mkdir -p "$PROBE_DEST"
cp "target/$TARGET/release/examples/command_backends_probe" "$PROBE_DEST/libcommand-backends-probe.so"
printf 'Device probe packaged: %s/libcommand-backends-probe.so\n' "$PROBE_DEST"
