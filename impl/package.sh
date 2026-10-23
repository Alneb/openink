#!/usr/bin/env bash
# Prebuilt openink for apps that build without Gradle (e.g. Atlas's build-apk.sh: javac + d8 + copy .so):
#   impl/package.sh   → impl/target/dist/
#     openink.jar               org.openink.* classes (javac --release 17 against android.jar; d8 them yourself)
#     onyx-compat.jar           com.onyx.* compatibility layer (docs/ONYX-COMPAT.md), compiled against openink.jar
#     arm64-v8a/libopenink_jni.so   stripped JNI library (stroke, pen, epd, shape)
#     VERSION, SHA256SUMS
# Tools as build-demo.sh: $OPENINK_TOOLCHAINS, else the Atlas project's copy (docs/OPEN-STACK.md, "Build").
set -euo pipefail
impl=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
tc=${OPENINK_TOOLCHAINS:-$impl/.toolchains}
jar=$(ls "$tc"/unpack/*/android.jar | head -1)
strip=${ANDROID_NDK_HOME:-$tc/android-ndk-r30}/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip
for p in "$jar" "$strip"; do [ -e "$p" ] || { echo "[package] FAILED: missing $p" >&2; exit 1; }; done
dist=$impl/target/dist; work=$impl/target/dist-work
rm -rf "$dist" "$work"; mkdir -p "$dist/arm64-v8a" "$work/openink" "$work/compat"

echo "[package] 1/3 libopenink_jni.so for arm64 (cargo, NDK)"
"$impl/build-android.sh" -p openink-jni
"$strip" -o "$dist/arm64-v8a/libopenink_jni.so" "$impl/target/aarch64-linux-android/release/libopenink_jni.so"

echo "[package] 2/3 openink.jar and onyx-compat.jar (javac --release 17, -Werror)"
mapfile -t lib < <(find "$impl/java/openink/src/main/java" -name '*.java')
javac --release 17 -Xlint:all -Werror -cp "$jar" -d "$work/openink" "${lib[@]}"
jar --create --file "$dist/openink.jar" -C "$work/openink" .
mapfile -t compat < <(find "$impl/java/onyx-compat/src/main/java" -name '*.java')
javac --release 17 -Xlint:all -Werror -cp "$jar:$dist/openink.jar" -d "$work/compat" "${compat[@]}"
jar --create --file "$dist/onyx-compat.jar" -C "$work/compat" .

echo "[package] 3/3 VERSION + SHA256SUMS"
{
  echo "openink $(git -C "$impl" describe --always --dirty)"
  echo "built $(date -u +%Y-%m-%dT%H:%M:%SZ) for arm64-v8a, Android API ${OPENINK_ANDROID_API:-35}"
} > "$dist/VERSION"
(cd "$dist" && sha256sum openink.jar onyx-compat.jar arm64-v8a/libopenink_jni.so VERSION > SHA256SUMS)
rm -rf "$work"
echo "[package] done: $dist"
ls -la "$dist" "$dist/arm64-v8a"
