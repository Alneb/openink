#!/usr/bin/env bash
# Cross-build the openink crates for the device (aarch64 Android). Usage: ./build-android.sh [cargo args]
# NDK: $ANDROID_NDK_HOME, else the copy the Atlas project carries (docs/OPEN-STACK.md, "Build").
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
ndk="${ANDROID_NDK_HOME:-${OPENINK_TOOLCHAINS:-$here/.toolchains}/android-ndk-r30}"
api="${OPENINK_ANDROID_API:-35}"   # the device runs Android 15 (API 35)
cc="$ndk/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android${api}-clang"
[[ -x "$cc" ]] || { echo "[build-android] FAILED: no NDK clang at $cc (set ANDROID_NDK_HOME)" >&2; exit 1; }
echo "[build-android] NDK $ndk, API $api"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$cc"
export CC_aarch64_linux_android="$cc"
cd "$here"
cargo build --target aarch64-linux-android --release "$@"
echo "[build-android] outputs in $here/target/aarch64-linux-android/release/"
