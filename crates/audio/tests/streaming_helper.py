#!/usr/bin/env python3
"""Protocol fixture only: counts PCM; does not perform speech recognition."""
import json
import struct
import sys


def emit(kind, **fields):
    print(json.dumps(dict(type=kind, **fields)), flush=True)


emit("ready", engine="analyzer", locale=sys.argv[2], streaming=True)
while True:
    header = sys.stdin.buffer.read(4)
    if not header:
        sys.exit(0)
    count, = struct.unpack("<I", header)
    if count == 0xFFFFFFFF:
        break
    if not count:
        sys.exit(0)
    samples = struct.unpack("<%df" % count, sys.stdin.buffer.read(count * 4))
    emit("text", text="Sentence: %d samples, sum %.1f" % (count, sum(samples)))

emit("streaming")
count, total, previous_second = 0, 0.0, -1
pending = b""
while True:
    data = sys.stdin.buffer.read1(6400)
    if not data:
        break
    pending += data
    size = len(pending) // 4 * 4
    samples = struct.unpack("<%df" % (size // 4), pending[:size])
    pending = pending[size:]
    count += len(samples)
    total += sum(samples)
    if count and count // 16000 != previous_second:
        emit("partial", text="%s temporary: %d samples" % (sys.argv[2], count), start=0, end=count / 16000)
        previous_second = count // 16000
assert not pending, "truncated float32"
if count:
    emit("final", text="Stream: %d samples, sum %.1f" % (count, total), start=0, end=count / 16000)
emit("done")
