#!/bin/sh
# Build the Mac app: target/package/boardcast-<version>-<arch>-macos.zip, holding boardcast.app.
# It is signed only ad hoc, which Apple silicon requires to run at all, so a downloaded copy has to
# be opened with Control-click the first time.
# Needs Rust, Node with `pnpm install` done, and network access the first time for models and llama-server.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"

node scripts/glyphs.mjs
node scripts/fetch-llama.mjs
node scripts/fetch-voice.mjs
node scripts/fetch-pdfium.mjs
cargo build --release -p boardcast-ui -p boardcast-tools

version=$(target/release/boardcast --version | cut -d' ' -f2)
name="boardcast-$version-$(uname -m)-macos"
out="target/package/$name"
rm -rf "$out"
app="$out/boardcast.app/Contents"
lib="$app/Resources/boardcast"
mkdir -p "$app/MacOS" "$lib/voice/sense-voice" "$lib/voice/speaker" "$lib/pdfium" "$lib/licenses"

install -m755 target/release/boardcast "$app/MacOS/boardcast"
strip "$app/MacOS/boardcast"
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist.in > "$app/Info.plist"
cp src-tauri/icons/icon.icns "$app/Resources/icon.icns"

target/release/boardcast-tools pack-glyphs public/hanzi "$lib/glyphs.pack"
cp -R src-tauri/llama "$lib/llama"
rm -f "$lib/llama/.version"
cp target/pdfium/libpdfium.dylib "$lib/pdfium/"
cp src-tauri/voice/silero_vad.onnx "$lib/voice/"
cp src-tauri/voice/speaker/campplus_zh_en.onnx "$lib/voice/speaker/"
cp src-tauri/voice/sense-voice/model.int8.onnx src-tauri/voice/sense-voice/tokens.txt "$lib/voice/sense-voice/"
cp LICENSE public/licenses/* public/hanzi/ARPHICPL.TXT "$lib/licenses/"
cp target/pdfium/LICENSE "$lib/licenses/pdfium-LICENSE"

codesign --force --deep --sign - "$out/boardcast.app"

"$app/MacOS/boardcast" --self-check
ditto -c -k --keepParent "$out/boardcast.app" "target/package/$name.zip"
ls -lh "target/package/$name.zip"
