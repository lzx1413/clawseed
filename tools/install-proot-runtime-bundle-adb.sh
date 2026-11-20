#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGE="${CLAWSEED_ANDROID_PACKAGE:-dev.clawseed.demo}"
BUNDLE="${1:?runtime bundle tar.gz is required}"
MANIFEST="${2:?runtime manifest json is required}"
APK="${3:-$ROOT/clients/android/app/build/outputs/apk/debug/app-debug.apk}"
SERIAL="${4:-${ADB_SERIAL:-}}"

if [[ -n "${ADB_BIN:-}" ]]; then
  ADB_PATH="$ADB_BIN"
elif command -v adb >/dev/null 2>&1; then
  ADB_PATH="$(command -v adb)"
else
  ADB_PATH="${ANDROID_HOME:-/root/Android/Sdk}/platform-tools/adb"
fi
[[ -x "$ADB_PATH" ]] || { printf 'adb not found: %s\n' "$ADB_PATH" >&2; exit 1; }
ADB=("$ADB_PATH")
if [[ -n "$SERIAL" ]]; then ADB+=( -s "$SERIAL" ); fi
[[ -f "$BUNDLE" && -f "$MANIFEST" && -f "$APK" ]] || {
  printf 'bundle, manifest, or APK is missing\n' >&2
  exit 1
}
"${ADB[@]}" get-state >/dev/null

RUNTIME_ID="$(python3 - "$MANIFEST" <<'PY'
import json, sys
print(json.load(open(sys.argv[1], encoding="utf-8"))["runtime_id"])
PY
)"
GENERATION="${RUNTIME_ID}-adb"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
printf '%s\n' "{\"runtime_id\":\"$RUNTIME_ID\",\"generation\":\"$GENERATION\"}" > "$WORK/active.json"
printf '%s\n' '{"state":"READY"}' > "$WORK/state.json"

REMOTE_ARCHIVE=/data/local/tmp/clawseed-proot-runtime.tar.gz
REMOTE_MANIFEST=/data/local/tmp/clawseed-proot-runtime-manifest.json
REMOTE_ACTIVE=/data/local/tmp/clawseed-proot-runtime-active.json
REMOTE_STATE=/data/local/tmp/clawseed-proot-runtime-state.json
REMOTE_ROOT=/data/local/tmp/clawseed-proot-runtime-root

"${ADB[@]}" install -r "$APK" >/dev/null
"${ADB[@]}" push "$BUNDLE" "$REMOTE_ARCHIVE" >/dev/null
"${ADB[@]}" push "$MANIFEST" "$REMOTE_MANIFEST" >/dev/null
"${ADB[@]}" push "$WORK/active.json" "$REMOTE_ACTIVE" >/dev/null
"${ADB[@]}" push "$WORK/state.json" "$REMOTE_STATE" >/dev/null
"${ADB[@]}" shell rm -rf "$REMOTE_ROOT"
"${ADB[@]}" shell run-as "$PACKAGE" rm -rf files/proot-runtime
"${ADB[@]}" shell run-as "$PACKAGE" mkdir -p "files/proot-runtime/versions/$GENERATION"
"${ADB[@]}" shell run-as "$PACKAGE" tar -xzf "$REMOTE_ARCHIVE" -C "files/proot-runtime/versions/$GENERATION"
"${ADB[@]}" shell run-as "$PACKAGE" cp "$REMOTE_MANIFEST" "files/proot-runtime/versions/$GENERATION/manifest.json"
"${ADB[@]}" shell run-as "$PACKAGE" cp "$REMOTE_ACTIVE" files/proot-runtime/active.json
"${ADB[@]}" shell run-as "$PACKAGE" cp "$REMOTE_STATE" files/proot-runtime/state.json
"${ADB[@]}" shell rm -f "$REMOTE_ARCHIVE" "$REMOTE_MANIFEST" "$REMOTE_ACTIVE" "$REMOTE_STATE"
"${ADB[@]}" shell am force-stop "$PACKAGE"

printf 'Installed real runtime generation: %s\n' "$GENERATION"
printf 'Open the app and test runtime_status/linux_exec in a chat.\n'
