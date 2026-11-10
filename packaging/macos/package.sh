#!/bin/sh
# Build the Mac apps: target/package/boardcast-<version>-<arch>-macos[-<variant>].zip, holding boardcast.app,
# one per variant in VARIANTS (default "full"; see packaging/models.sh). "apple" carries no speech models
# and recognizes with Apple's own SpeechAnalyzer, or SFSpeechRecognizer before macOS 26.
# They are signed only ad hoc, which Apple silicon requires to run at all, so a downloaded copy has to
# be opened with Control-click the first time.
# Needs Rust, Xcode, Node with `pnpm install` done, and network access the first time for models and llama-server.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
. packaging/models.sh
variants="${VARIANTS:-full}"

node scripts/glyphs.mjs
node scripts/fetch-llama.mjs
node scripts/fetch-voice.mjs
node scripts/fetch-pdfium.mjs
cargo build --release -p boardcast-ui -p boardcast-tools

version=$(target/release/boardcast --version | cut -d' ' -f2)
mkdir -p target/package
glyphs=target/package/glyphs.pack
target/release/boardcast-tools pack-glyphs public/hanzi "$glyphs"
speech=target/package/boardcast-speech
sh scripts/build-apple-speech.sh "$speech"

for variant in $variants; do
  name="boardcast-$version-$(uname -m)-macos$(variant_suffix "$variant")"
  out="target/package/$name"
  rm -rf "$out"
  app="$out/boardcast.app/Contents"
  lib="$app/Resources/boardcast"
  mkdir -p "$app/MacOS" "$lib/voice/speaker" "$lib/pdfium" "$lib/licenses"

  install -m755 target/release/boardcast "$app/MacOS/boardcast"
  strip "$app/MacOS/boardcast"
  install -m755 "$speech" "$app/MacOS/boardcast-speech"
  sed "s/@VERSION@/$version/g" packaging/macos/Info.plist.in > "$app/Info.plist"
  cp packaging/icons/icon.icns "$app/Resources/icon.icns"

  cp "$glyphs" "$lib/glyphs.pack"
  cp -R resources/llama "$lib/llama"
  rm -f "$lib/llama/.version"
  cp target/pdfium/libpdfium.dylib "$lib/pdfium/"
  cp resources/voice/silero_vad.onnx "$lib/voice/"
  cp resources/voice/speaker/campplus_zh_en.onnx "$lib/voice/speaker/"
  copy_models "$variant" "$lib/voice"
  printf '%s\n' "$variant" > "$lib/variant"
  cp LICENSE public/licenses/* public/hanzi/ARPHICPL.TXT "$lib/licenses/"
  cp target/pdfium/LICENSE "$lib/licenses/pdfium-LICENSE"

  codesign --force --deep --sign - "$out/boardcast.app"

  "$app/MacOS/boardcast" --self-check
  ditto -c -k --keepParent "$out/boardcast.app" "target/package/$name.zip"
  rm -rf "$out"
  ls -lh "target/package/$name.zip"
done
