#!/bin/sh
# Record your own voice reading eval/asr/own-voice/sentences.json, so recognition can be measured on
# the speech, microphone and words it will really meet. Recordings stay on this machine: the folder
# they go in is ignored by git.
#
#   sh eval/asr/own-voice/record.sh            # from the first sentence not yet recorded
#   sh eval/asr/own-voice/record.sh zh-05      # record (or redo) one sentence
#
# Each sentence: press Enter, read it the way you would say it in a meeting, press Enter again.
# Speak naturally; don't slow down or over-pronounce. Needs ffmpeg and a working default microphone.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
out="$here/recordings"
mkdir -p "$out"
only="${1:-}"
case "$(uname -s)" in
  Darwin) backend=avfoundation; input=none:default ;;
  Linux) backend=pulse; input=default ;;
  *) printf 'Recording is supported on macOS and Linux.\n' >&2; exit 1 ;;
esac

python3 -c 'import json, sys; [print(s["id"], s["text"], sep="\t") for s in json.load(open(sys.argv[1]))]' "$here/sentences.json" |
while IFS="$(printf '\t')" read -r id text; do
  [ -n "$only" ] && [ "$id" != "$only" ] && continue
  [ -z "$only" ] && [ -f "$out/$id.wav" ] && continue
  printf '\n[%s] %s\n' "$id" "$text"
  printf 'Enter to start… '
  read -r _ </dev/tty
  # A failed re-recording must not replace the previous take.
  take="$out/.$id.part.wav"
  ffmpeg -loglevel error -y -f "$backend" -i "$input" -ac 1 -ar 16000 -c:a pcm_s16le "$take" </dev/null &
  rec=$!
  trap 'if [ -n "$rec" ]; then kill -INT "$rec" 2>/dev/null || true; wait "$rec" 2>/dev/null || true; fi; rm -f "$take"' 0
  trap 'exit 1' HUP INT TERM
  printf 'Recording. Enter to stop… '
  read -r _ </dev/tty
  kill -INT "$rec" 2>/dev/null || true
  wait "$rec" 2>/dev/null || true
  rec=
  python3 - "$take" <<'PY'
import sys, wave
with wave.open(sys.argv[1], 'rb') as audio:
    assert audio.getnchannels() == 1 and audio.getframerate() == 16000
    assert audio.getsampwidth() == 2 and audio.getnframes() > 0, 'No audio recorded; check microphone permissions'
PY
  mv "$take" "$out/$id.wav"
  trap - 0 HUP INT TERM
  printf 'Saved %s\n' "$out/$id.wav"
done
echo
python3 -c 'from pathlib import Path; import sys; p=Path(sys.argv[1]); print(len(list(p.glob("*.wav"))), "recordings in", p)' "$out"
