#!/usr/bin/env bash
# Explicitly isolated audio fixture. Never opens a default or physical microphone.
set -eu
if pactl list short sinks | rg -q '[[:space:]]boardcast_preview_qa[[:space:]]'; then
  echo 'The QA sink already exists; remove that fixture before rerunning.' >&2
  exit 1
fi
preview_wav=$(mktemp /tmp/boardcast-preview-XXXXXX.wav)
preview_module=''
cleanup() {
  if [ -n "$preview_module" ]; then pactl unload-module "$preview_module"; fi
  rm "$preview_wav"
}
trap cleanup EXIT
preview_module=$(pactl load-module module-null-sink sink_name=boardcast_preview_qa sink_properties=device.description=BoardcastPreviewQA)
python3 - "$preview_wav" <<'PY'
import math, struct, sys, wave
with wave.open(sys.argv[1], 'wb') as output:
    output.setnchannels(1)
    output.setsampwidth(2)
    output.setframerate(48000)
    output.writeframes(b''.join(struct.pack('<h', round(6553 * math.sin(2 * math.pi * 440 * i / 48000))) for i in range(96000)))
PY
BOARDCAST_PREVIEW_TEST_DEVICE='Monitor of BoardcastPreviewQA' \
BOARDCAST_PREVIEW_TEST_WAV="$preview_wav" \
cargo test --manifest-path "$(dirname "$0")/../../src-tauri/Cargo.toml" virtual_input_meter --lib -- --ignored --nocapture
