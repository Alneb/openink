#!/usr/bin/env bash
# Build the openink test app without Gradle: cargo (JNI lib) → javac → d8 → aapt2 link → zipalign → apksigner.
#   impl/java/demo/build-demo.sh [--install]     → impl/target/demo/openink-demo.apk
# Tools come from $OPENINK_TOOLCHAINS, else the Atlas project's copy (docs/OPEN-STACK.md, "Build").
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
impl=$(cd "$here/../.." && pwd)
tc=${OPENINK_TOOLCHAINS:-$here/../../.toolchains}
bt=$(dirname "$(ls "$tc"/unpack/*/aapt2 | head -1)")   # build-tools: aapt2, d8, zipalign, apksigner
jar=$(ls "$tc"/unpack/*/android.jar | head -1)
strip=${ANDROID_NDK_HOME:-$tc/android-ndk-r30}/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip
for p in "$bt/aapt2" "$bt/d8" "$jar" "$strip"; do [ -e "$p" ] || { echo "[demo] FAILED: missing $p" >&2; exit 1; }; done
out=$impl/target/demo; work=$out/work; rm -rf "$work"; mkdir -p "$work/classes" "$work/apk/lib/arm64-v8a"

echo "[demo] 1/4 libopenink_jni.so for arm64 (cargo, NDK)"
"$impl/build-android.sh" -p openink-jni
"$strip" -o "$work/apk/lib/arm64-v8a/libopenink_jni.so" "$impl/target/aarch64-linux-android/release/libopenink_jni.so"

echo "[demo] 2/4 Java: the org.openink library, onyx-compat + the app (javac → d8)"
mapfile -t srcs < <(find "$impl/java/openink/src/main/java" "$impl/java/onyx-compat/src/main/java" "$here/src" -name '*.java')
javac --release 17 -Xlint:all -Werror -cp "$jar" -d "$work/classes" "${srcs[@]}"
"$bt/d8" --release --min-api 26 --lib "$jar" --output "$work/apk" $(find "$work/classes" -name '*.class')

echo "[demo] 3/4 link the manifest, add dex + lib, align"
"$bt/aapt2" link -o "$work/base.apk" --manifest "$here/AndroidManifest.xml" -I "$jar" \
  --min-sdk-version 26 --target-sdk-version 35 --version-code "$(( $(date +%s) / 60 ))" \
  --version-name "0.1.0-$(git -C "$impl" rev-parse --short HEAD)"
(cd "$work/apk" && zip -q -r "$work/base.apk" classes.dex lib)
"$bt/zipalign" -P 16 -f 4 "$work/base.apk" "$work/aligned.apk"

echo "[demo] 4/4 sign (a debug key of this machine, made once in impl/target/)"
key=$impl/target/openink-debug.keystore
[ -f "$key" ] || keytool -genkeypair -keystore "$key" -storepass openink -keypass openink -alias openink \
  -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=openink debug" >/dev/null 2>&1
"$bt/apksigner" sign --v4-signing-enabled false --ks "$key" --ks-pass pass:openink --key-pass pass:openink \
  --out "$out/openink-demo.apk" "$work/aligned.apk"
"$bt/apksigner" verify "$out/openink-demo.apk"
echo "[demo] $out/openink-demo.apk ($(du -h "$out/openink-demo.apk" | cut -f1))"
if [ "${1:-}" = --install ]; then
  adb install -r "$out/openink-demo.apk"
  adb shell am start -n org.openink.demo/.DemoActivity
fi
