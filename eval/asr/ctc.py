# /// script
# requires-python = ">=3.11,<3.14"
# dependencies = ["torch==2.9.1", "transformers==4.57.3", "numpy==2.2.6"]
# [tool.uv.sources]
# torch = { index = "pytorch-cpu" }
# [[tool.uv.index]]
# name = "pytorch-cpu"
# url = "https://download.pytorch.org/whl/cpu"
# explicit = true
# ///
"""Pinned CPU CTC controls on the desktop VAD's audio intervals; no text prompts."""
import json
import sys
import time
import wave
from pathlib import Path

import numpy as np
import torch
from transformers import AutoModelForCTC, AutoProcessor


def segment(audio, start, end):
    first, last = round(start * 16000), round(end * 16000)
    if not 0 <= first < last <= len(audio) + 1600:
        raise ValueError("Invalid VAD interval")
    # Desktop finish may pad the final VAD window with zeros.
    result = audio[first:min(last, len(audio))]
    return np.pad(result, (0, last - first - len(result)))


if sys.argv[1:] == ["--self-test"]:
    audio = np.arange(16000, dtype=np.float32)
    assert np.array_equal(segment(audio, 0.25, 0.5), audio[4000:8000])
    assert np.array_equal(segment(audio, 0.99, 1.01), np.concatenate([audio[15840:], np.zeros(160)]))
    try:
        segment(audio, -1, 0)
        raise AssertionError("Invalid interval accepted")
    except ValueError:
        pass
    print("CTC audio interval checks passed")
    sys.exit(0)

input_data = json.loads(Path(sys.argv[1]).read_text())
if (input_data["policy"], input_data["vad"], input_data["denoise"]) != ("static", "on", "none"):
    raise ValueError("CTC requires static VAD on without denoising")
torch.set_num_threads(2)
started = time.perf_counter()
processor = AutoProcessor.from_pretrained(input_data["external_model"], local_files_only=True, trust_remote_code=False)
model = AutoModelForCTC.from_pretrained(input_data["external_model"], local_files_only=True, trust_remote_code=False).eval()
load_ms = (time.perf_counter() - started) * 1000
output = Path(input_data["output"])
results = json.loads(output.read_text())
results["segmentationLoadMs"] = results["loadMs"]
results["loadMs"] = load_ms
results["backend"] = "transformers-ctc"
for sample, result in zip(input_data["samples"], results["samples"], strict=True):
    if sample["id"] != result["id"] or sample["variant"] != "original":
        raise ValueError("Mismatched CTC sample")
    with wave.open(sample["path"], "rb") as wav:
        if (wav.getnchannels(), wav.getsampwidth(), wav.getframerate()) != (1, 2, 16000):
            raise ValueError("Expected 16 kHz mono PCM16")
        audio = np.frombuffer(wav.readframes(wav.getnframes()), dtype="<i2").astype(np.float32) / 32768
    result["segmentationPassMs"] = result["elapsedMs"]
    started = time.perf_counter()
    for heard in result["segments"]:
        decode_start = time.perf_counter()
        samples = segment(audio, heard["start"], heard["end"])
        with torch.inference_mode():
            values = processor(samples, sampling_rate=16000, return_tensors="pt")
            ids = model(**values).logits.argmax(dim=-1)
        heard["text"] = processor.batch_decode(ids)[0].strip()
        heard["timing"]["asrMs"] = round((time.perf_counter() - decode_start) * 1000)
    result["elapsedMs"] = (time.perf_counter() - started) * 1000
    result["partials"] = 0
    result["trace"] = result["segments"]
    print(sample["id"], ":", " ".join(s["text"] for s in result["segments"]), flush=True)
output.write_text(json.dumps(results, ensure_ascii=False, indent=2) + "\n")
