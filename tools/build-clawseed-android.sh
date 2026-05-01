#!/usr/bin/env bash
# Cross-compile clawseed for Android (gateway-only mode).
# Usage: ./tools/build-clawseed-android.sh [aarch64|x86_64|armv7] [check|build]
#
# Output binary: target/<triple>/release/clawseed
# Packaged to:   clients/android/app/src/main/jniLibs/<abi>/libclawseed.so
# Android installs jniLibs to nativeLibraryDir (exec-allowed), unlike assets/filesDir.

set -euo pipefail

ARCH="${1:-aarch64}"
ACTION="${2:-build}"

NDK_ROOT="${ANDROID_NDK_ROOT:-${ANDROID_HOME:-/home/zuoxin/Android/Sdk}/ndk/29.0.14206865}"
NDK_BIN="${NDK_ROOT}/toolchains/llvm/prebuilt/linux-x86_64/bin"
FEATURES="android"

case "${ARCH}" in
    aarch64)
        TARGET="aarch64-linux-android"
        ABI="arm64-v8a"
        CC="${NDK_BIN}/aarch64-linux-android21-clang"
        ;;
    x86_64)
        TARGET="x86_64-linux-android"
        ABI="x86_64"
        CC="${NDK_BIN}/x86_64-linux-android21-clang"
        ;;
    armv7)
        TARGET="armv7-linux-androideabi"
        ABI="armeabi-v7a"
        CC="${NDK_BIN}/armv7a-linux-androideabi21-clang"
        ;;
    *)
        echo "Unknown arch: ${ARCH}. Use: aarch64 | x86_64 | armv7"
        exit 1
        ;;
esac

AR="${NDK_BIN}/llvm-ar"

echo "==> Building clawseed for ${TARGET} (${ABI}), action=${ACTION}"
echo "    NDK: ${NDK_ROOT}"

# Export compiler env vars for cc-rs (rusqlite --bundled, ring, etc.)
TARGET_UPPER="${TARGET^^}"
TARGET_VAR="${TARGET_UPPER//-/_}"
export "CC_${TARGET_VAR}=${CC}"
export "AR_${TARGET_VAR}=${AR}"
export "CARGO_TARGET_${TARGET_VAR}_LINKER=${CC}"
export CC="${CC}"
export AR="${AR}"
export ANDROID_NDK_ROOT="${NDK_ROOT}"

CARGO_ARGS=(
    "${ACTION}"
    -p clawseed
    --target "${TARGET}"
    --no-default-features
    --features "${FEATURES}"
)
[[ "${ACTION}" == "build" ]] && CARGO_ARGS+=(--release)

cargo "${CARGO_ARGS[@]}"

if [[ "${ACTION}" == "build" ]]; then
    BINARY="target/${TARGET}/release/clawseed"
    # Place as .so in jniLibs — Android installs jniLibs to nativeLibraryDir which is exec-allowed
    JNI_DIR="clients/android/app/src/main/jniLibs/${ABI}"
    mkdir -p "${JNI_DIR}"
    cp "${BINARY}" "${JNI_DIR}/libclawseed.so"
    SIZE=$(wc -c < "${JNI_DIR}/libclawseed.so")
    echo "==> Packaged: ${JNI_DIR}/libclawseed.so (${SIZE} bytes)"
fi
