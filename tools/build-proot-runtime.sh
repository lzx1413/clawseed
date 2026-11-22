#!/usr/bin/env bash
set -euo pipefail

# Build an offline Ubuntu + Node.js/npm + CPython/pip runtime for Android.
# PRoot inputs come from Termux; guest interpreters are Linux/glibc builds.
# This is an independent ClawSeed recipe; no DSH engine or bridge is included.

ABI="${1:-arm64-v8a}"
OUT="${2:-build/proot-runtime-arm64}"
BASE_URL="${3:-https://example.com/clawseed/proot}"
PROOT_VERSION="${PROOT_VERSION:-5.1.107.102}"
NODE_VERSION="24.22.1"
PYTHON_VERSION="3.12.15"
PYTHON_RELEASE="20261021"
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
command -v xz >/dev/null || { printf 'xz is required\n' >&2; exit 1; }
command -v sha256sum >/dev/null || { printf 'sha256sum is required\n' >&2; exit 1; }
command -v python3 >/dev/null || { printf 'python3 is required\n' >&2; exit 1; }

CACHE="${CACHE_DIR:-$OUT/cache}"
STAGE="$OUT/stage"
ARCHIVE="$OUT/clawseed-proot-runtime-$ABI.tar.xz"
MANIFEST="$OUT/manifest.json"
RUNTIME_ID="ubuntu-$UBUNTU_VERSION-node-$NODE_VERSION-python-$PYTHON_VERSION-$ABI-r2"

rm -rf "$STAGE"
mkdir -p "$CACHE" "$STAGE/rootfs" "$STAGE/proot-libs" "$STAGE/licenses" "$OUT"

fetch() {
  local url="$1" destination="$2" expected="${3:-}"
  mkdir -p "$(dirname "$destination")"
  if [[ ! -f "$destination" ]]; then
    curl -fL --retry 3 --connect-timeout 20 -sS -o "$destination.part" "$url" || return 1
    mv "$destination.part" "$destination" || return 1
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
  fetch "$TERMUX_REPO/$filename" "$destination" "$sha" || return 1
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
  "${PROOT_SHA256:-10a394b585c2fc2b2c8502544f4e5f1be581dc8bfff79e8ba62f16207423e030}")"
talloc_deb="$(download_termux_deb libtalloc "$TALLOC_VERSION" \
  "${TALLOC_SHA256:-556591f43bb773ad8777e1a29522640866a55f95dab71914418b94a8c58ad5a7}")"
shmem_deb="$(download_termux_deb libandroid-shmem "$SHMEM_VERSION" \
  "${SHMEM_SHA256:-0da3a24d558b93c92bcf8d611e0826a99ff96e396b148e6cdf33b47c47c57ff6}")"

extract_deb "$proot_deb" "$OUT/proot-package"
extract_deb "$talloc_deb" "$OUT/talloc-package"
extract_deb "$shmem_deb" "$OUT/shmem-package"

cp "$OUT/proot-package/data/data/com.termux/files/usr/bin/proot" "$STAGE/proot"
cp "$OUT/proot-package/data/data/com.termux/files/usr/libexec/proot/loader" "$STAGE/proot-loader"
cp -L "$OUT/talloc-package/data/data/com.termux/files/usr/lib/libtalloc.so.$TALLOC_VERSION" "$STAGE/proot-libs/libtalloc.so.2"
cp "$OUT/shmem-package/data/data/com.termux/files/usr/lib/libandroid-shmem.so" "$STAGE/proot-libs/libandroid-shmem.so"
chmod 0755 "$STAGE/proot" "$STAGE/proot-loader"

node_archive="node-v$NODE_VERSION-linux-arm64.tar.xz"
python_archive="cpython-$PYTHON_VERSION+$PYTHON_RELEASE-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz"
fetch "https://nodejs.org/dist/v$NODE_VERSION/$node_archive" "$CACHE/$node_archive" \
  dbabf7a1cbf947e6059ffad2c27b80041db97dea1e553b62afdee7f13fca336e
fetch "https://github.com/astral-sh/python-build-standalone/releases/download/$PYTHON_RELEASE/$python_archive" \
  "$CACHE/$python_archive" b4567aa5a11f51b540c4dfd9e082ebf23e554f8ac17b85caa6cea23e83693394
mkdir -p "$STAGE/rootfs/opt/node" "$STAGE/rootfs/opt/python" "$STAGE/rootfs/usr/local/bin" \
  "$STAGE/rootfs/workspace" "$STAGE/rootfs/root/.l2s" "$STAGE/rootfs/etc/ssl/certs"
tar -xJf "$CACHE/$node_archive" --strip-components=1 -C "$STAGE/rootfs/opt/node"
tar -xzf "$CACHE/$python_archive" --strip-components=1 -C "$STAGE/rootfs/opt/python"
# Keep interpreter, standard library, pip, npm and headers needed by native extensions.
for command in node npm npx; do
  ln -s "../../../opt/node/bin/$command" "$STAGE/rootfs/usr/local/bin/$command"
done
# CPython records sys._base_executable in pyvenv.cfg. A symlink under
# /usr/local/bin makes copy-mode venv look for stdlib there, then fall back to
# the standalone build prefix /install. exec the actual installation path.
for command in python python3; do
  cat > "$STAGE/rootfs/usr/local/bin/$command" <<'PYLAUNCHER'
#!/bin/sh
exec /opt/python/bin/python3 "$@"
PYLAUNCHER
  chmod 0755 "$STAGE/rootfs/usr/local/bin/$command"
done
for command in pip pip3; do
  ln -s "../../../opt/python/bin/$command" "$STAGE/rootfs/usr/local/bin/$command"
done
# pip's pinned distribution includes a Mozilla CA bundle. Reuse those exact bytes
# for Python's ssl and system clients; there is no build-host certificate input.
cp "$STAGE/rootfs/opt/python/lib/python3.12/site-packages/pip/_vendor/certifi/cacert.pem" \
  "$STAGE/rootfs/etc/ssl/certs/ca-certificates.crt"
ln -sf certs/ca-certificates.crt "$STAGE/rootfs/etc/ssl/cert.pem"
printf 'nameserver 1.1.1.1\nnameserver 8.8.8.8\n' > "$STAGE/rootfs/etc/resolv.conf"
cat > "$STAGE/licenses/SOURCE-METADATA.txt" <<EOF
PRoot: $PROOT_VERSION ($TERMUX_REPO), GPL-2.0
libtalloc: $TALLOC_VERSION (Termux), LGPL-3.0-or-later
libandroid-shmem: $SHMEM_VERSION (Termux), BSD-2-Clause
Ubuntu Base: $ubuntu_base (copyright files in rootfs/usr/share/doc)
Node.js: $NODE_VERSION (license in rootfs/opt/node/LICENSE)
CPython standalone: $PYTHON_VERSION+$PYTHON_RELEASE (licenses in rootfs/opt/python)
CA bundle: pip vendored certifi in the hash-pinned CPython distribution
ClawSeed runtime: no DSH engine or bridge.
EOF
# Retain native upstream notices alongside the redistributable bundle.
cp "$OUT/shmem-package/data/data/com.termux/files/usr/share/doc/libandroid-shmem/copyright" \
  "$STAGE/licenses/libandroid-shmem.txt"
sed -n '1,/\*\//p' "$OUT/talloc-package/data/data/com.termux/files/usr/include/talloc.h" \
  > "$STAGE/licenses/libtalloc.txt"
fetch "https://raw.githubusercontent.com/termux/proot/v$PROOT_VERSION/COPYING" "$CACHE/proot-COPYING"
cp "$CACHE/proot-COPYING" "$STAGE/licenses/proot-COPYING.txt"

# Dereference hardlinks only; symlinks remain guest paths. Bound XZ dictionary
# size so Android can stream-decode with modest memory.
tar --hard-dereference -c -C "$STAGE" proot proot-loader proot-libs rootfs licenses | xz -T2 -6 > "$ARCHIVE"
python3 - "$MANIFEST" "$ARCHIVE" "$BASE_URL" "$ABI" "$RUNTIME_ID" "$NODE_VERSION" "$PYTHON_VERSION" <<'PYMANIFEST'
import hashlib
import json
import pathlib
import sys
import tarfile
manifest_path, archive_path, base_url, abi, runtime_id, node, python = sys.argv[1:]
archive = pathlib.Path(archive_path)
with tarfile.open(archive) as contents:
    unpacked = sum(entry.size for entry in contents if entry.isfile())
manifest = {
    "schema_version": 1,
    "runtime_version": runtime_id,
    "runtime_id": runtime_id,
    "abi": abi,
    "min_android": 26,
    "archive_size": archive.stat().st_size,
    "unpacked_size": unpacked,
    "archive_format": "tar.xz",
    "archive_url": base_url.rstrip("/") + "/" + archive.name,
    "archive_sha256": hashlib.file_digest(archive.open("rb"), "sha256").hexdigest(),
    "publisher_key_id": "clawseed-runtime",
    "components": {"node": node, "python": python},
}
pathlib.Path(manifest_path).write_text(json.dumps(manifest, indent=2) + "\n")
print(f"Runtime archive: {archive.stat().st_size / 1048576:.2f} MiB; unpacked files: {unpacked / 1048576:.2f} MiB")
PYMANIFEST
printf 'Runtime ready: %s\nManifest: %s\n' "$ARCHIVE" "$MANIFEST"
