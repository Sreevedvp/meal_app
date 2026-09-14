#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
: "${JAVA_HOME:?Set JAVA_HOME to JDK 17 or newer}"
: "${ANDROID_HOME:?Set ANDROID_HOME to the Android SDK}"
: "${ANDROID_NDK_HOME:?Set ANDROID_NDK_HOME to an installed Android NDK}"
DX_BIN="${DX_BIN:-dx}"
if [[ "$DX_BIN" == dx && -x work/tools/dx ]]; then DX_BIN="$PWD/work/tools/dx"; fi
exec "$DX_BIN" build --platform android -p meal-app --locked
