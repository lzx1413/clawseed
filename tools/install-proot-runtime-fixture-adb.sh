#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGE="${CLAWSEED_ANDROID_PACKAGE:-dev.clawseed.demo}"
APK="${1:-$ROOT/clients/android/app/build/outputs/apk/debug/app-debug.apk}"
SERIAL="${2:-${ADB_SERIAL:-}}"
if [[ -n "${ADB_BIN:-}" ]]; then
  ADB_PATH="$ADB_BIN"
elif command -v adb >/dev/null 2>&1; then
  ADB_PATH="$(command -v adb)"
else
  ADB_PATH="${ANDROID_HOME:-/root/Android/Sdk}/platform-tools/adb"
fi
[[ -x "$ADB_PATH" ]] || { printf 'adb not found: %s\n' "$ADB_PATH" >&2; exit 1; }
ADB=("$ADB_PATH")
if [[ -n "$SERIAL" ]]; then
  ADB+=( -s "$SERIAL" )
fi

[[ -f "$APK" ]] || { printf 'APK not found: %s\n' "$APK" >&2; exit 1; }
"${ADB[@]}" get-state >/dev/null

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
FIXTURE="$WORK/fixture"
"$ROOT/tools/build-proot-runtime-fixture.sh" arm64-v8a "$FIXTURE" http://unused.local

GENERATION="fixture-arm64-v8a-1-adb"
GENERATION_DIR="$WORK/$GENERATION"
mkdir -p "$GENERATION_DIR"
tar -xzf "$FIXTURE/clawseed-proot-fixture-arm64-v8a.tar.gz" -C "$GENERATION_DIR"
cp "$FIXTURE/manifest.json" "$GENERATION_DIR/manifest.json"
chmod 0755 "$GENERATION_DIR/proot"

"${ADB[@]}" install -r "$APK" >/dev/null
REMOTE_SOURCE="/data/local/tmp/clawseed-runtime-generation"
"${ADB[@]}" shell rm -rf "$REMOTE_SOURCE"
"${ADB[@]}" push "$GENERATION_DIR/." "$REMOTE_SOURCE" >/dev/null
printf '%s\n' '{"runtime_id":"fixture-arm64-v8a-1","generation":"'"$GENERATION"'"}' > "$WORK/active.json"
printf '%s\n' '{"state":"READY"}' > "$WORK/state.json"
"${ADB[@]}" push "$WORK/active.json" /data/local/tmp/clawseed-runtime-active.json >/dev/null
"${ADB[@]}" push "$WORK/state.json" /data/local/tmp/clawseed-runtime-state.json >/dev/null
"${ADB[@]}" shell run-as "$PACKAGE" rm -rf files/proot-runtime
"${ADB[@]}" shell run-as "$PACKAGE" mkdir -p files/proot-runtime/versions
"${ADB[@]}" shell run-as "$PACKAGE" mkdir -p "files/proot-runtime/versions/$GENERATION/rootfs/etc"
"${ADB[@]}" shell run-as "$PACKAGE" cp \
  "$REMOTE_SOURCE/proot" \
  "files/proot-runtime/versions/$GENERATION/proot"
"${ADB[@]}" shell run-as "$PACKAGE" cp \
  "$REMOTE_SOURCE/manifest.json" \
  "files/proot-runtime/versions/$GENERATION/manifest.json"
"${ADB[@]}" shell run-as "$PACKAGE" cp \
  "$REMOTE_SOURCE/rootfs/etc/os-release" \
  "files/proot-runtime/versions/$GENERATION/rootfs/etc/os-release"
"${ADB[@]}" shell run-as "$PACKAGE" chmod 0755 "files/proot-runtime/versions/$GENERATION/proot"
"${ADB[@]}" shell run-as "$PACKAGE" cp /data/local/tmp/clawseed-runtime-active.json files/proot-runtime/active.json
"${ADB[@]}" shell run-as "$PACKAGE" cp /data/local/tmp/clawseed-runtime-state.json files/proot-runtime/state.json
"${ADB[@]}" shell rm -rf "$REMOTE_SOURCE"
"${ADB[@]}" shell rm -f /data/local/tmp/clawseed-runtime-active.json /data/local/tmp/clawseed-runtime-state.json
"${ADB[@]}" shell am force-stop "$PACKAGE"

printf 'Installed APK: %s\n' "$APK"
printf 'Seeded runtime generation: %s\n' "$GENERATION"
printf 'Open the app and use runtime_status/linux_exec in a chat.\n'
