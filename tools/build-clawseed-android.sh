#!/usr/bin/env bash
# Cross-compile clawseed for Android (gateway-only mode).
# Usage: ./tools/build-clawseed-android.sh [aarch64|x86_64|armv7] [check|build]
#
# Output binary: target/<triple>/release/clawseed
# Packaged to:   clients/android/app/src/main/jniLibs/<abi>/libclawseed.so
# ONNX Runtime:  clients/android/app/src/main/jniLibs/<abi>/libonnxruntime.so
# Android installs jniLibs to nativeLibraryDir (exec-allowed), unlike assets/filesDir.
# ort uses load-dynamic (runtime dlopen) to avoid GNU libc/Bionic symbol conflicts.

set -euo pipefail

ARCH="${1:-aarch64}"
ACTION="${2:-build}"

NDK_ROOT="${ANDROID_NDK_ROOT:-${ANDROID_HOME:-/home/zuoxin/Android/Sdk}/ndk/29.0.14206865}"
NDK_BIN="${NDK_ROOT}/toolchains/llvm/prebuilt/linux-x86_64/bin"
FEATURES="android,local-embedding"
ORT_AAR_URL="https://repo1.maven.org/maven2/com/microsoft/onnxruntime/onnxruntime-android/1.24.2/onnxruntime-android-1.24.2.aar"

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
CXX="${CC}++"

echo "==> Building clawseed for ${TARGET} (${ABI}), action=${ACTION}"
echo "    NDK: ${NDK_ROOT}"
echo "    Features: ${FEATURES}"

# Export compiler env vars for cc-rs (rusqlite --bundled, ring, etc.)
TARGET_UPPER="${TARGET^^}"
TARGET_VAR="${TARGET_UPPER//-/_}"
export "CC_${TARGET_VAR}=${CC}"
export "AR_${TARGET_VAR}=${AR}"
export "CXX_${TARGET_VAR}=${CXX}"
export "CARGO_TARGET_${TARGET_VAR}_LINKER=${CC}"
export CC="${CC}"
export AR="${AR}"
export CXX="${CXX}"
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

    # Download and bundle ONNX Runtime shared library for load-dynamic mode
    # Use Microsoft's official AAR (contains .so for all ABIs) instead of ort-sys CDN
    # (ort-sys CDN archive has only static .a for Android targets)
    ORT_CACHE_DIR="${HOME}/.cache/ort-android/${TARGET}"
    ORT_SO="${ORT_CACHE_DIR}/libonnxruntime.so"

    if [[ ! -f "${ORT_SO}" ]]; then
        echo "==> Downloading ONNX Runtime AAR for Android"
        mkdir -p "${ORT_CACHE_DIR}"
        ORT_AAR="${ORT_CACHE_DIR}/ort.aar"
        curl -sL "${ORT_AAR_URL}" -o "${ORT_AAR}"

        # Extract .so from AAR (it's a zip containing jni/{ABI}/libonnxruntime.so)
        unzip -o "${ORT_AAR}" "jni/${ABI}/libonnxruntime.so" -d "${ORT_CACHE_DIR}/extracted" 2>/dev/null
        FOUND_SO="${ORT_CACHE_DIR}/extracted/jni/${ABI}/libonnxruntime.so"
        if [[ -f "${FOUND_SO}" ]]; then
            cp "${FOUND_SO}" "${ORT_SO}"
            echo "==> ONNX Runtime .so extracted: ${ORT_SO}"
        else
            echo "==> WARNING: libonnxruntime.so not found in AAR for ABI ${ABI}"
            echo "    Embedding will fall back to keyword-only search on this device."
        fi

        rm -f "${ORT_AAR}"
        rm -rf "${ORT_CACHE_DIR}/extracted"
    fi

    if [[ -f "${ORT_SO}" ]]; then
        cp "${ORT_SO}" "${JNI_DIR}/libonnxruntime.so"
        ORT_SIZE=$(wc -c < "${JNI_DIR}/libonnxruntime.so")
        echo "==> Packaged: ${JNI_DIR}/libonnxruntime.so (${ORT_SIZE} bytes)"
    fi
fi