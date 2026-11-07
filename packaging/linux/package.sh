#!/bin/sh
# Build the Linux package: target/package/boardcast-<version>-<arch>-linux.tar.gz.
# It unpacks into a working app, and install.sh inside puts it on the system.
# Needs Rust, Node with `pnpm install` done, and network access the first time for models and llama-server.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"

node scripts/glyphs.mjs
node scripts/fetch-llama.mjs linux-x64
node scripts/fetch-voice.mjs
node scripts/fetch-pdfium.mjs
cargo build --release -p boardcast-ui -p boardcast-tools

version=$(target/release/boardcast --version | cut -d' ' -f2)
name="boardcast-$version-$(uname -m)-linux"
out="target/package/$name"
rm -rf "$out"
lib="$out/lib/boardcast"
mkdir -p "$out/bin" "$lib/voice/sense-voice" "$out/share/licenses/boardcast"

install -m755 target/release/boardcast "$out/bin/boardcast"
strip "$out/bin/boardcast"

target/release/boardcast-tools pack-glyphs public/hanzi "$lib/glyphs.pack"
cp -r src-tauri/llama "$lib/llama"
mkdir -p "$lib/pdfium"
cp target/pdfium/libpdfium.so "$lib/pdfium/"
rm -f "$lib/llama/.version"
cp src-tauri/voice/silero_vad.onnx "$lib/voice/"
mkdir -p "$lib/voice/speaker"
cp src-tauri/voice/speaker/campplus_zh_en.onnx "$lib/voice/speaker/"
cp src-tauri/voice/sense-voice/model.int8.onnx src-tauri/voice/sense-voice/tokens.txt "$lib/voice/sense-voice/"

cp LICENSE public/licenses/* public/hanzi/ARPHICPL.TXT "$out/share/licenses/boardcast/"
cp target/pdfium/LICENSE "$out/share/licenses/boardcast/pdfium-LICENSE"
install -m644 src-tauri/icons/128x128@2x.png "$out/share/boardcast.png"
install -m644 packaging/linux/boardcast.desktop.in "$out/share/boardcast.desktop.in"
install -m755 packaging/linux/install.sh "$out/install.sh"
printf '%s\n' "$version" > "$out/VERSION"

"$out/bin/boardcast" --self-check
tar -C target/package -czf "target/package/$name.tar.gz" "$name"
ls -lh "target/package/$name.tar.gz"
