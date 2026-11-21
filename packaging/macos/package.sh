#!/bin/sh
# Build the Mac apps: target/package/alcyo-<version>-<arch>-macos[-<variant>].zip, holding Alcyo.app,
# one per variant in VARIANTS (default "all"; see packaging/models.sh). "none" carries no speech models
# and recognizes with Apple's own SpeechAnalyzer, or SFSpeechRecognizer before macOS 26.
# They are signed only ad hoc, which Apple silicon requires to run at all, so a downloaded copy has to
# be opened with Control-click the first time.
# Needs Rust, Xcode, and network access the first time for models and llama-server.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
. packaging/models.sh
variants="${VARIANTS:-all}"

cargo build --release -p boardcast-tools
for what in glyphs "llama" voice search pdfium; do
  target/release/boardcast-tools fetch $what
done
cargo build --release -p boardcast-ui

version=$(target/release/alcyo --version | cut -d' ' -f2)
mkdir -p target/package
glyphs=target/package/glyphs.pack
target/release/boardcast-tools pack-glyphs assets/hanzi "$glyphs"
speech=target/package/alcyo-speech
sh scripts/build-apple-speech.sh "$speech"

for variant in $variants; do
  name="alcyo-$version-$(uname -m)-macos$(variant_suffix "$variant")"
  out="target/package/$name"
  rm -rf "$out"
  app="$out/Alcyo.app/Contents"
  lib="$app/Resources/alcyo"
  mkdir -p "$app/MacOS" "$lib/voice/speaker" "$lib/pdfium" "$lib/licenses"

  install -m755 target/release/alcyo "$app/MacOS/alcyo"
  strip "$app/MacOS/alcyo"
  install -m755 "$speech" "$app/MacOS/alcyo-speech"
  sed "s/@VERSION@/$version/g" packaging/macos/Info.plist.in > "$app/Info.plist"
  cp packaging/icons/icon.icns "$app/Resources/icon.icns"

  cp "$glyphs" "$lib/glyphs.pack"
  cp -R resources/llama "$lib/llama"
  rm -f "$lib/llama/.version"
  cp target/pdfium/libpdfium.dylib "$lib/pdfium/"
  cp resources/voice/silero_vad.onnx "$lib/voice/"
  cp resources/voice/speaker/campplus_zh_en.onnx "$lib/voice/speaker/"
  cp resources/voice/speaker/nemotron-3-diarization.int8.onnx "$lib/voice/speaker/"
  copy_models "$variant" "$lib/voice"
  copy_search "$lib/search"
  printf '%s\n' "$variant" > "$lib/variant"
  cp LICENSE assets/licenses/* crates/fonts/files/OFL-*.txt assets/hanzi/ARPHICPL.TXT "$lib/licenses/"
  # The fonts built into the program (crates/fonts) go with their licenses.
  for f in OFL-DMSans.txt OFL-JetBrainsMono.txt OFL-NotoSansCJK.txt; do test -f "$lib/licenses/$f" || { echo "missing font license $f" >&2; exit 1; }; done
  cp target/pdfium/LICENSE "$lib/licenses/pdfium-LICENSE"

  codesign --force --deep --sign - "$out/Alcyo.app"

  "$app/MacOS/alcyo" --self-check
  ditto -c -k --keepParent "$out/Alcyo.app" "target/package/$name.zip"
  rm -rf "$out"
  ls -lh "target/package/$name.zip"
done
