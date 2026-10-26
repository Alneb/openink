#!/usr/bin/env bash
# Every gate for impl/, in one place: ./check.sh [--no-device-builds]  (skips stages 4–5)
# Each stage is named and logged; the first failure stops the run with a non-zero exit.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here"
tc=${OPENINK_TOOLCHAINS:-$here/.toolchains}   # the Android NDK, android.jar and build-tools: set OPENINK_TOOLCHAINS or link impl/.toolchains
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
stage() { echo; echo "[check] === $1 ==="; }

stage "1 Rust tests (host)"
cargo test --workspace --quiet

stage "2 clippy, warnings are errors"
cargo clippy --workspace --all-targets --quiet -- -D warnings

stage "3 Java host checks: SpecCheck (protocol spec), ScreenMapCheck (raw → screen fit), MapCalibratorCheck, ShapeWireCheck, HwrWireCheck"
lib=java/openink/src/main/java/org/openink
test=java/openink/src/test/java
javac --release 11 -Xlint:all -Werror -d "$work/host" \
  "$lib"/{epd/Waveform,epd/UpdateMode,sf/SfCode,pen/ScreenMap,pen/PenAxes,pen/PenReader,pen/MapCalibrator,shape/Shape,shape/ShapeClass,hwr/HandwritingRecognizer}.java \
  "$test"/android/util/Log.java "$test"/org/openink/{SpecCheck,ScreenMapCheck}.java "$test"/org/openink/pen/MapCalibratorCheck.java \
  "$test"/org/openink/shape/ShapeWireCheck.java "$test"/org/openink/hwr/HwrWireCheck.java
java -cp "$work/host" org.openink.SpecCheck spec/sf_protocol.json
java -cp "$work/host" org.openink.ScreenMapCheck
java -cp "$work/host" org.openink.pen.MapCalibratorCheck
java -cp "$work/host" org.openink.shape.ShapeWireCheck
java -cp "$work/host" org.openink.hwr.HwrWireCheck

if [ "${1:-}" = --no-device-builds ]; then echo; echo "[check] PASS (device builds skipped)"; exit 0; fi

stage "4 Android: Rust cross-build + the demo APK (javac -Werror against android.jar)"
java/demo/build-demo.sh

stage "5 onyx-compat compiles; demo host checks: PageFile round trip, ShapeSnap, compat TouchPointList + RegionGate (android.jar on the classpath: classes load, no Android calls)"
aj=$(ls "$tc"/unpack/*/android.jar | head -1)
javac --release 17 -Xlint:all -Werror -cp "$aj" -d "$work/demo" \
  $(find java/openink/src/main/java java/onyx-compat/src/main/java java/demo/src -name '*.java') java/demo/test/org/openink/demo/PageFileCheck.java \
  java/onyx-compat/src/test/java/org/openink/compat/pen/TouchPointListCheck.java java/demo/test/org/openink/demo/ShapeSnapCheck.java \
  java/onyx-compat/src/test/java/org/openink/compat/pen/RegionGateCheck.java
java -cp "$work/demo:$aj" org.openink.demo.PageFileCheck
java -cp "$work/demo:$aj" org.openink.demo.ShapeSnapCheck
java -cp "$work/demo:$aj" org.openink.compat.pen.TouchPointListCheck
java -cp "$work/demo:$aj" org.openink.compat.pen.RegionGateCheck

echo; echo "[check] PASS"
