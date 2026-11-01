# Speech evaluation

Download the shared audio and open its local listening page:

```sh
pnpm eval:audio
# Open eval/asr/.data/index.html directly, or download and serve it in one command:
pnpm eval:listen
# http://127.0.0.1:4175 — Ctrl+C stops the server
```

Node.js 24+ and `curl` are sufficient for downloading/listening; the page also works offline through `file://`. No Hugging Face account, API key, Python, ffmpeg, Rust or running desktop app is required for listening. The download command verifies every cached file and repairs missing/corrupt files using the fixed [asr-fixtures-v1 Release](https://github.com/HsiangNianian/boardcast/releases/tag/asr-fixtures-v1). Release files must never be replaced in place: publish a new version and update the hashes instead.

`corpus.json` is the shared manifest for downloads, playback and scoring. WAV files live in ignored `.data/`; optional model files live in ignored `.models/`. Source IDs, original intervals, license links and reference quality travel with each sample. See [LICENSES.md](LICENSES.md) before redistributing audio or transcripts. Neither the excluded MagicData recordings nor personal recordings are uploaded.

## Run recognition

ASR evaluation additionally needs Rust and the same [Tauri system dependencies](https://v2.tauri.app/start/prerequisites/) as desktop development (including `libasound2-dev` on Ubuntu). It downloads the required models automatically, without starting a GUI or opening a microphone:

```sh
pnpm eval:asr                                      # 17 quick cases, application SenseVoice int8
pnpm eval:asr --suite full                         # all 30 samples, including long meetings
pnpm eval:asr --model whisper-base                 # multilingual Whisper base int8 CPU baseline
pnpm eval:asr --denoise gtcrn                       # same inputs, with offline GTCRN denoising
pnpm eval:asr --suite full --model whisper-base --denoise gtcrn
pnpm eval:asr --vad on                            # production segmentation (default)
pnpm eval:asr --vad off                           # no VAD; fixed 10-second windows, including silence
pnpm eval:asr --only ja-fleurs-00,en-ami-00          # debug individual quick cases
```

With `--vad on`, the runner calls the ignored Rust corpus test explicitly and uses the **same `voice::Engine`** as the desktop: Silero VAD, soft breaks, pre-roll, final recognition and voiceprint computation. Quiet input gets gradual, bounded gain (up to 8×) in the VAD's copy only; the recognizer and voiceprints receive the audio before that gain. Soft pauses are relative to the speaker's level. The Whisper comparison changes only the recognizer inside the test; it does not add a new production setting. Both recognizers use automatic language detection. Model files have pinned SHA-256 checksums; Whisper URLs also pin the upstream revision.

`--vad off` bypasses VAD, its gain, soft breaks and pre-roll. It sends **every** sample, including silence, through the same recognizer and voiceprint extractor in fixed windows of at most 10 seconds. This bounds memory on long meetings and keeps the Whisper comparison within its input window, but may split words. It is a segmentation ablation, not a new application setting or a promise that whole-recording recognition is better. Reports and artifact names always include the VAD mode.

GTCRN runs before segmentation, so the comparison includes its effects on VAD and ASR. This is offline denoising, not an implemented real-time enhancement or overlapping-speaker separation feature. The corpus retains original noise, level and overlap; it does not inject synthetic noise.

Audio is fed in 100 ms chunks **faster than real time**. RTF measures total processing time divided by audio duration; model loading is reported separately. This is not microphone-to-screen latency and does not measure the frontend, physical sound devices or echo cancellation. Partial-result counts depend on wall-clock scheduling and are not an accuracy gate.

## Corpus and scores

The full pack contains 30 WAVs, about 26 MiB / 14 minutes (including overlapping short/long excerpts of the same meetings). It is a small regression fixture, not a representative multilingual benchmark.

| Group | Samples | Reference treatment |
| --- | ---: | --- |
| Chinese AliMeeting | 1 long meeting + 2 short turns | Community-derived text remains provisional. Short-turn CER is shown separately; excluded from reference aggregates. The long overlapping meeting is transcript-only. |
| English AMI | 1 long meeting + 5 isolated turns | Official manual references. Ordinary WER only on isolated turns; the long overlapping meeting is transcript-only. |
| Japanese FLEURS | 12 separate utterances | Publisher test-set references; **human read speech, not Japanese meetings**. CER on each recording. |
| Chinese-English ASCEND | 8 short clips | Publisher references from one session. MER uses individual Han characters plus Latin word/number tokens. Technical-term recall is reported where annotated. |
| Silence | 1 generated control | Any final recognized text is flagged; it fails the production-pipeline run. Ablations retain the failure as a reported measurement. |

The quick suite contains 2 Chinese turns, 3 English turns, 3 Japanese utterances, all 8 mixed-language probes and silence. Full evaluation includes everything. Selecting `--only` is recorded in the report and should not be compared with a full-suite aggregate.

Normalization is fixed in `score.mjs`: Unicode NFKC, lowercase, ignore punctuation/whitespace; English apostrophes and AMI acronym underscores are removed. Chinese/Japanese use Unicode characters, English uses words, mixed speech uses Han characters and Latin words. Numbers are not converted between spoken forms and digits; kanji/kana and simplified/traditional variants are not equated. These differences can affect scores. Error rates may exceed 100% when there are many insertions.

Rates are micro-averaged by reference units within a language, **not** averaged across languages. `provisional` and `unscored` samples are excluded from those totals; an absent score is shown as `—`, not zero errors. The overlapping meeting transcripts are not flattened into a falsely authoritative word order. No CER/WER merge threshold is enforced until references and expected variance have been checked.

Each run writes `input.json`, `hypotheses.json`, `report.json` and `report.md` under `eval/reports/asr/`, including corpus/version/hash, model hashes, engine hash, Git commit, working-tree status, VAD mode, input RMS/peak level, detected VAD segments/seconds, final segment count, hypotheses, reference text, CER/WER/MER, keyword recall and RTF. On GitHub, the Markdown also appears in the job summary. Input level is measured before denoising; VAD duration includes segment padding. These help distinguish low-level input, missed detection and empty recognition.

The production pipeline (SenseVoice, no GTCRN, VAD on) now fails on **any** empty speech transcript or speech hallucinated on silence, even when other cases succeed. Other combinations are experimental controls: the same checks are marked PASS/FAIL prominently in their reports and emit CI warnings, but do not block on expected ablation failures such as silence hallucinations without VAD. Download, checksum, configuration, inference and missing-result errors still fail **every** configuration. CER/WER remain measured without an accuracy threshold.

The [initial four-way run](baseline.md) records the first full-corpus results and their limits.

## One workflow

[`.github/workflows/eval.yml`](../../.github/workflows/eval.yml) is the single testing workflow:

- Pushes to `main` and pull requests run type checks, unit tests, web build, four desktop builds with Rust tests, the existing text-to-board evaluation, and quick SenseVoice ASR evaluation with **VAD on and off** (two jobs).
- Every Monday at 02:30 UTC, or manual **Run workflow → full_asr**, the ASR matrix runs the full corpus with SenseVoice / Whisper base × raw / GTCRN × VAD on / off (**eight jobs**). Scheduled runs omit desktop packaging and the board model evaluation.
- Audio and models use separate caches keyed by their manifests. Restored files are always verified. Reports are uploaded as per-model artifacts for 30 days; audio/model caches are only accelerators, not the permanent source of the fixtures.

The data Release uses an `asr-fixtures-*` tag and is **not Latest**, so it does not trigger the `v*` software release workflow or replace the application's update manifest. Pages deployment and software releases remain separate workflows.
