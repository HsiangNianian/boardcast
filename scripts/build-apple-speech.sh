#!/bin/sh
set -eu
cd "$(dirname "$0")/../src-tauri"
target="$1"
case "$target" in
  aarch64-apple-darwin) arch=arm64 ;;
  x86_64-apple-darwin) arch=x86_64 ;;
  *) echo "Unsupported Apple speech target: $target" >&2; exit 1 ;;
esac
xcrun swiftc -O -parse-as-library -swift-version 5 -target "$arch-apple-macos11.0" \
  apple-speech/main.swift \
  -Xlinker -sectcreate -Xlinker __TEXT -Xlinker __info_plist -Xlinker apple-speech/Info.plist \
  -o "apple-speech/boardcast-speech-$target"
