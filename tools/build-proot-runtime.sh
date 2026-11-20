#!/usr/bin/env bash
set -euo pipefail

# Builds a ClawSeed-only arm64 runtime from the public Termux PRoot packages
# and Ubuntu Base. It intentionally does not install DSH, Node.js, Python, or
# any DSH bridge. The resulting bundle is suitable for adb preloading first.

ABI="${1:-arm64-v8a}"
OUT="${2:-build/proot-runtime-arm64}"
BASE_URL="${3:-https://example.com/clawseed/proot}"
PROOT_VERSION="${PROOT_VERSION:-5.1.107.102}"
TALLOC_VERSION="2.5.0"
SHMEM_VERSION="0.7"
UBUNTU_VERSION="24.04.5"
TERMUX_REPO="https://packages.termux.dev/apt/termux-main"
UBUNTU_BASE_URL="https://mirrors.tuna.tsinghua.edu.cn/ubuntu-cdimage/ubuntu-base/releases/24.04/release"

[[ "$ABI" == "arm64-v8a" ]] || {
  printf 'Only arm64-v8a is supported by this builder.\n' >&2
  exit 2
}

command -v curl >/dev/null || { printf 'curl is required\n' >&2; exit 1; }
command -v ar >/dev/null || { printf 'ar is required\n' >&2; exit 1; }
command -v tar >/dev/null || { printf 'tar is required\n' >&2; exit 1; }
command -v sha256sum >/dev/null || { printf 'sha256sum is required\n' >&2; exit 1; }
command -v python3 >/dev/null || { printf 'python3 is required\n' >&2; exit 1; }

CACHE="${CACHE_DIR:-$OUT/cache}"
STAGE="$OUT/stage"
ARCHIVE="$OUT/clawseed-proot-runtime-$ABI.tar.gz"
MANIFEST="$OUT/manifest.json"
RUNTIME_ID="ubuntu-$UBUNTU_VERSION-$ABI-proot-$PROOT_VERSION"

rm -rf "$STAGE"
mkdir -p "$CACHE" "$STAGE/rootfs" "$STAGE/proot-libs" "$STAGE/licenses" "$OUT"

fetch() {
  local url="$1" destination="$2" expected="${3:-}"
  mkdir -p "$(dirname "$destination")"
  if [[ ! -f "$destination" ]]; then
    curl -fL --retry 3 --connect-timeout 20 -sS -o "$destination.part" "$url"
    mv "$destination.part" "$destination"
  fi
  if [[ -n "$expected" ]]; then
    local actual
    actual="$(sha256sum "$destination" | cut -d' ' -f1)"
    [[ "$actual" == "$expected" ]] || {
      rm -f "$destination"
      printf 'SHA-256 mismatch for %s: got %s want %s\n' "$destination" "$actual" "$expected" >&2
      exit 1
    }
  fi
}

ubuntu_base="ubuntu-base-${UBUNTU_VERSION}-base-arm64.tar.gz"
fetch "$UBUNTU_BASE_URL/$ubuntu_base" "$CACHE/$ubuntu_base" \
  a91d5a93010193712d346d761372b7c9db6dfcf093893161c64ca107f05914f2
tar -xzf "$CACHE/$ubuntu_base" -C "$STAGE/rootfs"

packages="$CACHE/Packages-aarch64"
fetch "$TERMUX_REPO/dists/stable/main/binary-aarch64/Packages" "$packages"

download_termux_deb() {
  local package="$1" version="$2" pin="$3"
  local record filename sha destination
  record="$(awk -v RS= -v p="$package" -v v="$version" \
    '$0 ~ "(^|\\n)Package: " p "\\n" && $0 ~ "\\nVersion: " v "\\n"' "$packages")"
  [[ -n "$record" ]] || { printf 'Termux package not found: %s %s\n' "$package" "$version" >&2; exit 1; }
  filename="$(sed -n 's/^Filename: //p' <<<"$record")"
  sha="$(sed -n 's/^SHA256: //p' <<<"$record")"
  [[ -z "$pin" || "$sha" == "$pin" ]] || { printf 'Unexpected Termux hash for %s\n' "$package" >&2; exit 1; }
  destination="$CACHE/$(basename "$filename")"
  fetch "$TERMUX_REPO/$filename" "$destination" "$sha"
  printf '%s' "$destination"
}

extract_deb() {
  local package="$1" destination="$2"
  local work="$OUT/deb-$(basename "$package")"
  rm -rf "$work"
  mkdir -p "$work" "$destination"
  ar x "$package" --output "$work"
  tar -xf "$work"/data.tar.* -C "$destination"
}

proot_deb="$(download_termux_deb proot "$PROOT_VERSION" \
  "${PROOT_SHA256:-}")"
talloc_deb="$(download_termux_deb libtalloc "$TALLOC_VERSION" \
  "${TALLOC_SHA256:-}")"
shmem_deb="$(download_termux_deb libandroid-shmem "$SHMEM_VERSION" \
  "${SHMEM_SHA256:-}")"

extract_deb "$proot_deb" "$OUT/proot-package"
extract_deb "$talloc_deb" "$OUT/talloc-package"
extract_deb "$shmem_deb" "$OUT/shmem-package"

cp "$OUT/proot-package/data/data/com.termux/files/usr/bin/proot" "$STAGE/proot"
cp "$OUT/proot-package/data/data/com.termux/files/usr/libexec/proot/loader" "$STAGE/proot-loader"
cp -L "$OUT/talloc-package/data/data/com.termux/files/usr/lib/libtalloc.so.$TALLOC_VERSION" "$STAGE/proot-libs/libtalloc.so.2"
cp "$OUT/shmem-package/data/data/com.termux/files/usr/lib/libandroid-shmem.so" "$STAGE/proot-libs/libandroid-shmem.so"
chmod 0755 "$STAGE/proot" "$STAGE/proot-loader"

cat > "$STAGE/licenses/SOURCE-METADATA.txt" <<EOF
PRoot version: $PROOT_VERSION
Termux repository: $TERMUX_REPO
Ubuntu Base: $ubuntu_base
This bundle contains PRoot and its native libraries only. It does not contain DSH, Node.js, Python, or a DSH bridge.
Preserve the upstream licenses before distributing this bundle.
EOF

# Android's toybox tar cannot create hardlinks inside an app's run-as sandbox.
# Materialize hardlinked files in the archive while preserving symlinks.
tar --hard-dereference -czf "$ARCHIVE" -C "$STAGE" proot proot-loader proot-libs rootfs licenses
archive_size="$(wc -c < "$ARCHIVE" | tr -d ' ')"
archive_sha256="$(sha256sum "$ARCHIVE" | cut -d' ' -f1)"
python3 - "$MANIFEST" "$BASE_URL" "$ABI" "$RUNTIME_ID" "$archive_size" "$archive_sha256" <<'PY'
import json
import sys

manifest_path, base_url, abi, runtime_id, archive_size, archive_sha256 = sys.argv[1:]
manifest = {
    "schema_version": 1,
    "runtime_version": runtime_id,
    "runtime_id": runtime_id,
    "abi": abi,
    "min_android": 26,
    "archive_size": int(archive_size),
    "archive_format": "tar.gz",
    "archive_url": base_url.rstrip("/") + f"/clawseed-proot-runtime-{abi}.tar.gz",
    "archive_sha256": archive_sha256,
    "publisher_key_id": "development-real-proot",
}
with open(manifest_path, "w", encoding="utf-8") as output:
    json.dump(manifest, output, indent=2)
    output.write("\n")
PY

printf 'Real PRoot runtime ready:\n  manifest: %s\n  archive:  %s\n' "$MANIFEST" "$ARCHIVE"
