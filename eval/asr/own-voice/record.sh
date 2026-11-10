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

python3 -c 'import json, sys; [print(s["id"], s["text"], sep="\t") for s in json.load(open(sys.argv[1]))]' "$here/sentences.json" |
while IFS="$(printf '\t')" read -r id text; do
  [ -n "$only" ] && [ "$id" != "$only" ] && continue
  [ -z "$only" ] && [ -f "$out/$id.wav" ] && continue
  printf '\n[%s] %s\n' "$id" "$text"
  printf 'Enter to start… '
  read -r _ </dev/tty
  ffmpeg -loglevel error -y -f pulse -i default -ac 1 -ar 16000 "$out/$id.wav" </dev/null &
  rec=$!
  printf 'Recording. Enter to stop… '
  read -r _ </dev/tty
  kill -INT "$rec"
  wait "$rec" 2>/dev/null || true
  printf 'Saved %s\n' "$out/$id.wav"
done
echo
ls "$out" | wc -l | xargs printf '%s recordings in %s\n' 2>/dev/null || true
