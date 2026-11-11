# Your own voice

The shared corpus is other people's recordings. This set is 30 sentences read by whoever uses boardcast,
on their own microphone: 20 in Chinese with the English terms a technical meeting carries, including
board commands, and 10 in English. It measures recognition where it matters, and compares engines on
the same audio.

```sh
sh eval/asr/own-voice/record.sh        # about ten minutes; stops and resumes at any sentence
sh eval/asr/own-voice/record.sh zh-05  # redo one sentence, keeping the old take if capture fails
```

Install `ffmpeg` and Python 3 first. The script uses the default microphone through PulseAudio/PipeWire on Linux
and AVFoundation on macOS; allow Terminal microphone access on the Mac. Each Enter starts or stops one take.

After recording all 30 sentences, compare the production recognizer with glossary correction off and on:

```sh
cargo run --release -p boardcast-tools -- fetch voice
cargo run --release -p boardcast-asr-eval -- own-voice .
```

The vocabulary is in `glossary.txt`; keep it fixed before recording and comparing. Each file is recognized once,
then the app's correction runs on those same final sentences. `eval/reports/own-voice/report.md` reports errors by
sample and language, including changes that make recognition worse. The adjacent JSON retains both transcripts,
recognizer identities and audio hashes for inspection. Missing recordings fail before models are loaded; an empty
recognition fails after writing the report. A successful command means the comparison ran, not that the glossary
improved every sentence. Report the actual errors and harmful changes before accepting #117.

Recordings go to `recordings/`, which git ignores, so a voice never leaves the machine. `sentences.json`
is the reference text. Chinese is scored by character, English by word, after removing punctuation and case.
Neither recordings nor generated reports are committed. This set covers Chinese and English, not Japanese speech;
the 45 synthetic pronunciation fixtures separately cover all three languages.

Recorder checks without opening a microphone: `python3 eval/asr/own-voice/test_record.py`.
