#!/usr/bin/env bash
# Build the Rust library and APK, install it and launch it.
#
#   PHONE=<adb serial or ip:port> scripts/deploy.sh
#
# Expects the Android SDK/NDK and a JDK 17; override the defaults below with
# ANDROID_SDK, ANDROID_NDK_HOME and JAVA_HOME as needed. Gradle comes from the
# wrapper in android/ (set GRADLE to use another one).
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SDK="${ANDROID_SDK:-$LOCALAPPDATA/Android/Sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$SDK/ndk/27.0.12077973}"
export JAVA_HOME="${JAVA_HOME:-$LOCALAPPDATA/Android/jdk-17.0.20.1+1}"
GRADLE="${GRADLE:-./gradlew}"
ADB="$SDK/platform-tools/adb"
[ -n "${PHONE:-}" ] && ADB="$ADB -s $PHONE"
LIB=libzcode_gpui_android.so
STRIP="$(ls -d "$ANDROID_NDK_HOME"/toolchains/llvm/prebuilt/*/bin/llvm-strip* | head -1)"

cd "$ROOT" || exit 1
if ! timeout 1200 cargo ndk -t arm64-v8a -P 26 build >target/ndk-build.log 2>&1; then
  grep -E "^error" -A12 target/ndk-build.log | head -80
  exit 1
fi
mkdir -p android/app/src/main/jniLibs/arm64-v8a
"$STRIP" --strip-debug -o "android/app/src/main/jniLibs/arm64-v8a/$LIB" \
  "target/aarch64-linux-android/debug/$LIB" || exit 1
(cd android && timeout 600 "$GRADLE" assembleDebug -q 2>&1 | tail -5)
timeout 120 $ADB install -r android/app/build/outputs/apk/debug/app-debug.apk | tail -1
timeout 5 $ADB logcat -c
timeout 5 $ADB shell am force-stop dev.zcode.gpui.mobile
timeout 10 $ADB shell am start -n dev.zcode.gpui.mobile/.MainActivity | tail -1
