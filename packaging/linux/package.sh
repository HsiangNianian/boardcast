#!/bin/sh
# Build the Linux packages: target/package/alcyo-<version>-<arch>-linux[-<variant>].tar.gz, one per
# variant in VARIANTS (default "all"; see packaging/models.sh). Each unpacks into a working app, and
# install.sh inside puts it on the system.
# Needs Rust, and network access the first time for models and llama-server.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
. packaging/models.sh
variants="${VARIANTS:-all}"

cargo build --release -p alcyo-tools
for what in glyphs "llama linux-x64" voice search pdfium; do
  target/release/alcyo-tools fetch $what
done
cargo build --release -p alcyo-ui

version=$(target/release/alcyo --version | cut -d' ' -f2)
glyphs=target/package/glyphs.pack
mkdir -p target/package
target/release/alcyo-tools pack-glyphs assets/hanzi "$glyphs"

for variant in $variants; do
  [ "$variant" = none ] && { echo "skipping none: Linux has no system recognizer" >&2; continue; }
  name="alcyo-$version-$(uname -m)-linux$(variant_suffix "$variant")"
  out="target/package/$name"
  rm -rf "$out"
  lib="$out/lib/boardcast"
  mkdir -p "$out/bin" "$lib/voice/speaker" "$lib/pdfium" "$out/share/licenses/boardcast"

  # Still bin/boardcast and lib/boardcast inside: installs from before the rename can only update to a
  # package laid out the way they expect. Their updater then learns the new layout for the next release.
  install -m755 target/release/alcyo "$out/bin/boardcast"
  strip "$out/bin/boardcast"

  cp "$glyphs" "$lib/glyphs.pack"
  cp -r resources/llama "$lib/llama"
  rm -f "$lib/llama/.version"
  cp target/pdfium/libpdfium.so "$lib/pdfium/"
  cp resources/voice/silero_vad.onnx "$lib/voice/"
  cp resources/voice/speaker/campplus_zh_en.onnx "$lib/voice/speaker/"
  cp resources/voice/speaker/nemotron-3-diarization.int8.onnx "$lib/voice/speaker/"
  copy_models "$variant" "$lib/voice"
  copy_search "$lib/search"
  printf '%s\n' "$variant" > "$lib/variant"

  cp LICENSE assets/licenses/* crates/fonts/files/OFL-*.txt assets/hanzi/ARPHICPL.TXT "$out/share/licenses/boardcast/"
  # The fonts built into the program (crates/fonts) go with their licenses.
  for f in OFL-DMSans.txt OFL-JetBrainsMono.txt OFL-NotoSansCJK.txt; do test -f "$out/share/licenses/boardcast/$f" || { echo "missing font license $f" >&2; exit 1; }; done
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
