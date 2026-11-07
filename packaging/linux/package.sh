#!/bin/sh
# Build the Linux packages: target/package/boardcast-<version>-<arch>-linux[-<variant>].tar.gz, one per
# variant in VARIANTS (default "full"; see packaging/models.sh). Each unpacks into a working app, and
# install.sh inside puts it on the system.
# Needs Rust, Node with `pnpm install` done, and network access the first time for models and llama-server.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
. packaging/models.sh
variants="${VARIANTS:-full}"

node scripts/glyphs.mjs
node scripts/fetch-llama.mjs linux-x64
node scripts/fetch-voice.mjs
node scripts/fetch-pdfium.mjs
cargo build --release -p boardcast-ui -p boardcast-tools

version=$(target/release/boardcast --version | cut -d' ' -f2)
glyphs=target/package/glyphs.pack
mkdir -p target/package
target/release/boardcast-tools pack-glyphs public/hanzi "$glyphs"

for variant in $variants; do
  [ "$variant" = apple ] && { echo "skipping apple: Linux has no system recognizer" >&2; continue; }
  name="boardcast-$version-$(uname -m)-linux$(variant_suffix "$variant")"
  out="target/package/$name"
  rm -rf "$out"
  lib="$out/lib/boardcast"
  mkdir -p "$out/bin" "$lib/voice/speaker" "$lib/pdfium" "$out/share/licenses/boardcast"

  install -m755 target/release/boardcast "$out/bin/boardcast"
  strip "$out/bin/boardcast"

  cp "$glyphs" "$lib/glyphs.pack"
  cp -r resources/llama "$lib/llama"
  rm -f "$lib/llama/.version"
  cp target/pdfium/libpdfium.so "$lib/pdfium/"
  cp resources/voice/silero_vad.onnx "$lib/voice/"
  cp resources/voice/speaker/campplus_zh_en.onnx "$lib/voice/speaker/"
  copy_models "$variant" "$lib/voice"
  printf '%s\n' "$variant" > "$lib/variant"

  cp LICENSE public/licenses/* public/hanzi/ARPHICPL.TXT "$out/share/licenses/boardcast/"
  cp target/pdfium/LICENSE "$out/share/licenses/boardcast/pdfium-LICENSE"
  install -m644 packaging/icons/128x128@2x.png "$out/share/boardcast.png"
  install -m644 packaging/linux/boardcast.desktop.in "$out/share/boardcast.desktop.in"
  install -m755 packaging/linux/install.sh "$out/install.sh"
  printf '%s\n' "$version" > "$out/VERSION"

  "$out/bin/boardcast" --self-check
  tar -C target/package -czf "target/package/$name.tar.gz" "$name"
  rm -rf "$out"
  ls -lh "target/package/$name.tar.gz"
done
