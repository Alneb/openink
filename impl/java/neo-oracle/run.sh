#!/usr/bin/env bash
# Build the oracle dex (javac → d8) and run it on the connected Boox: run.sh PEN_TYPE SAMPLES.txt [field=value ...]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
tc=${OPENINK_TOOLCHAINS:-$here/../../.toolchains}
bt=$(dirname "$(ls "$tc"/unpack/*/d8 | head -1)")
jar=$(ls "$tc"/unpack/*/android.jar | head -1)
work="$here/../../target/neo-oracle"
rm -rf "$work" && mkdir -p "$work/classes"
javac -q --release 11 -cp "$jar" -d "$work/classes" $(find "$here/src" -name '*.java') 2>/dev/null || javac --release 11 -cp "$jar" -d "$work/classes" $(find "$here/src" -name '*.java')
"$bt/d8" --release --min-api 26 --lib "$jar" --output "$work" $(find "$work/classes" -name '*.class')
adb shell "rm -rf /data/local/tmp/neo-bm; mkdir -p /data/local/tmp/neo-bm"
adb push -q "$work/classes.dex" /data/local/tmp/neo-oracle.dex
adb push -q "$2" /data/local/tmp/neo-samples.txt
pen="$1"; shift 2
adb shell CLASSPATH=/data/local/tmp/neo-oracle.dex app_process /data/local/tmp org.openink.oracle.Main "$pen" /data/local/tmp/neo-samples.txt "$@"
