#!/bin/sh
# Build boardcast-speech, the helper that recognizes speech with Apple's SpeechAnalyzer or SFSpeechRecognizer.
# Usage: scripts/build-apple-speech.sh <output> [arm64|x86_64]
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
out="$1"
arch="${2:-$(uname -m)}"
xcrun swiftc -O -parse-as-library -swift-version 5 -target "$arch-apple-macos13.0" \
  "$root/crates/audio/apple/speech.swift" \
  -Xlinker -sectcreate -Xlinker __TEXT -Xlinker __info_plist -Xlinker "$root/crates/audio/apple/Info.plist" \
  -o "$out"
"$out" self-test
