#!/usr/bin/env bash
set -euo pipefail

ABI="${1:-arm64-v8a}"
OUT="${2:-build/proot-runtime-fixture}"
BASE_URL="${3:-http://10.0.2.2:8765}"

case "$ABI" in
  arm64-v8a) ;;
  *)
    printf 'This development fixture only supports arm64-v8a, got %s\n' "$ABI" >&2
    exit 2
    ;;
esac

ROOT="$OUT/stage"
ARCHIVE="$OUT/clawseed-proot-fixture-$ABI.tar.gz"
MANIFEST="$OUT/manifest.json"
RUNTIME_ID="fixture-$ABI-1"

rm -rf "$OUT"
mkdir -p "$ROOT/rootfs/etc" "$OUT"

cat > "$ROOT/proot" <<'EOF'
#!/system/bin/sh
# Development-only launcher. It intentionally runs the command through the
# Android host shell and does not provide a Linux userspace or isolation.
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-c" ]; then
    shift
    exec /system/bin/sh -c "${1:-}"
  fi
  shift
done
exit 64
EOF
chmod 0755 "$ROOT/proot"

cat > "$ROOT/rootfs/etc/os-release" <<'EOF'
NAME="ClawSeed development fixture"
ID=clawseed-fixture
VERSION_ID="0"
PRETTY_NAME="ClawSeed host-shell fixture (not Linux)"
EOF

tar -czf "$ARCHIVE" -C "$ROOT" proot rootfs
ARCHIVE_SIZE="$(wc -c < "$ARCHIVE" | tr -d ' ')"
ARCHIVE_SHA256="$(sha256sum "$ARCHIVE" | cut -d' ' -f1)"

python3 - "$MANIFEST" "$BASE_URL" "$ABI" "$RUNTIME_ID" "$ARCHIVE_SIZE" "$ARCHIVE_SHA256" <<'PY'
import json
import sys

manifest_path, base_url, abi, runtime_id, archive_size, archive_sha256 = sys.argv[1:]
manifest = {
    "schema_version": 1,
    "runtime_version": "fixture-0",
    "runtime_id": runtime_id,
    "abi": abi,
    "min_android": 26,
    "archive_size": int(archive_size),
    "archive_format": "tar.gz",
    "archive_url": base_url.rstrip("/") + f"/clawseed-proot-fixture-{abi}.tar.gz",
    "archive_sha256": archive_sha256,
    "publisher_key_id": "development-fixture",
}
with open(manifest_path, "w", encoding="utf-8") as output:
    json.dump(manifest, output, indent=2)
    output.write("\n")
PY

printf 'Fixture ready:\n  manifest: %s\n  archive:  %s\n' "$MANIFEST" "$ARCHIVE"
printf 'Serve with: python3 -m http.server 8765 --directory %s\n' "$OUT"
