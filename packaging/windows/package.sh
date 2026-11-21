#!/bin/sh
# Build the Windows packages: target/package/alcyo-<version>-x86_64-windows[-<variant>].zip, one per
# variant in VARIANTS (default "all"; see packaging/models.sh). Each unpacks into a working app: run
# alcyo\bin\alcyo.exe. There is no installer and no in-app update yet.
# Runs in Git Bash, with Rust and network access the first time for models and llama-server.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
. packaging/models.sh
variants="${VARIANTS:-all}"

cargo build --release -p alcyo-tools
for what in glyphs "llama windows-x64" voice search "pdfium win-x64"; do
  target/release/alcyo-tools.exe fetch $what
done
cargo build --release -p alcyo-ui

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
glyphs=target/package/glyphs.pack
mkdir -p target/package
target/release/alcyo-tools.exe pack-glyphs assets/hanzi "$glyphs"

for variant in $variants; do
  [ "$variant" = none ] && { echo "skipping none: Windows has no system recognizer here" >&2; continue; }
  name="alcyo-$version-x86_64-windows$(variant_suffix "$variant")"
  out="target/package/$name"
  rm -rf "$out"
  lib="$out/lib/alcyo"
  mkdir -p "$out/bin" "$lib/voice/speaker" "$lib/pdfium" "$out/licenses"

  cp target/release/alcyo.exe "$out/bin/"
  # Any library a dependency's build left beside the program, such as the speech runtime.
  for dll in target/release/*.dll; do
    [ -e "$dll" ] && cp "$dll" "$out/bin/"
  done

  cp "$glyphs" "$lib/glyphs.pack"
  cp -r resources/llama "$lib/llama"
  rm -f "$lib/llama/.version"
  cp target/pdfium/pdfium.dll "$lib/pdfium/"
  cp resources/voice/silero_vad.onnx "$lib/voice/"
  cp resources/voice/speaker/campplus_zh_en.onnx "$lib/voice/speaker/"
  cp resources/voice/speaker/nemotron-3-diarization.int8.onnx "$lib/voice/speaker/"
  copy_models "$variant" "$lib/voice"
  copy_search "$lib/search"
  printf '%s\n' "$variant" > "$lib/variant"

  cp LICENSE assets/licenses/* crates/fonts/files/OFL-*.txt assets/hanzi/ARPHICPL.TXT "$out/licenses/"
  # The fonts built into the program (crates/fonts) go with their licenses.
  for f in OFL-DMSans.txt OFL-JetBrainsMono.txt OFL-NotoSansCJK.txt; do test -f "$out/licenses/$f" || { echo "missing font license $f" >&2; exit 1; }; done
  cp target/pdfium/LICENSE "$out/licenses/pdfium-LICENSE"
  printf '%s\n' "$version" > "$out/VERSION"

  "$out/bin/alcyo.exe" --self-check
  # Windows' own tar writes zip files.
  (cd target/package && "$SYSTEMROOT/System32/tar.exe" -a -c -f "$name.zip" "$name")
  rm -rf "$out"
  ls -lh "target/package/$name.zip"
done
