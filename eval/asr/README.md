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

## Long meetings across industries

```sh
pnpm eval:audio --suite meetings    # download, verify, crop; requires ffmpeg
pnpm eval:listen --suite meetings   # same offline HTML player, audio + corresponding reference
pnpm eval:asr --suite meetings
pnpm eval:asr --suite meetings --policy dynamic
pnpm eval:asr:sweep --suite meetings --models sensevoice,paraformer-zh-en,reazonspeech,parakeet-unified-en,qwen3-asr-0.6b,whisper-small
```

`meetings.json` adds **nine continuous excerpts of about ten minutes each**, plus silence. These are independent clips, never concatenated short utterances or synthetic speech. The suite is separate from `quick` / `full`, so push regression stays small.

The [2026-11-02 long-meeting results](long-meetings-2026-11-02.md) compare six candidate models and paired dynamic SenseVoice, including runtime failures and results by industry.

| Language | Recordings | Domains | Reference treatment |
| --- | --- | --- | --- |
| Chinese | 3 AISHELL-4 recordings | Property management, kindergarten education, factory safety | Human-recorded **scenario meetings**, not verified everyday business meetings. Official manual TextGrids; overlapping turns receive diagnostic ordered CER. |
| English | 4 Earnings-22 conference calls | Texas Instruments / semiconductors, Costco / retail, KKR / finance, Imperial Oil / energy | Actual public business calls. Publisher manual references with forced alignment; ordinary WER. |
| English | 1 AMI IB4001 excerpt | Research-space planning | Actual non-scenario meeting. Different interval of the existing session, not an independent new session. Diagnostic ordered WER for overlap. |
| Japanese | 1 Ministry of the Environment web committee | Public health / asbestos relief policy | Actual meeting on 2023-03-31. Official **edited minutes**, not a verbatim gold transcript; provisional CER only. |

Every entry records the original interval, recording ID, industry, meeting kind, source URLs/hashes, reference timing policy and transformations. Chinese uses microphone channel 0; AMI uses the official mixed headset; MP3 sources become mono 16 kHz PCM16. No denoising, gain or silence removal is baked into these clips. The Japanese clip boundary was located using a preliminary ASR pass; its text comes entirely from the official minutes. There has been no fresh human transcript audit.

Chinese and AMI overlap is preserved. `ordered` concatenates manual turns by start time for a diagnostic score; another valid ordering can add errors. It is **not** overlap-aware meeting WER/CER, diarization accuracy or a speaker-separation benchmark, and does not enter formal reference aggregates. `provisional` minutes are likewise separate. English Earnings-22 keeps unaligned lexical words, removes explicitly marked nonlexical tags, and uses the publisher's spoken-form normalization; literal WER here is not their `fstalign` equivalence score. Unintelligible words and spoken-number/digit differences can affect results.

The [asr-meetings-v2 data Release](https://github.com/HsiangNianian/boardcast/releases/tag/asr-meetings-v2) publishes the current manifest and notice; it reuses the unchanged three AISHELL-4 WAVs and AMI WAV from [v1](https://github.com/HsiangNianian/boardcast/releases/tag/asr-meetings-v1). Earnings-22's license explicitly covers text, so its audio is downloaded directly from the pinned upstream Git revision. Japanese audio is also fetched directly from the ministry, whose temporary publication may be withdrawn. A missing/changed source fails the download; it is never silently omitted. Neither source's audio is uploaded in our Release. Original and converted files are SHA-256 checked; local conversion uses output-side ffmpeg bitexact flags to omit encoder-version metadata (identical converted WAVs checked with ffmpeg 6.0 and 9.0.1). A decoder/version mismatch fails explicitly instead of comparing different audio unnoticed.

Only this long suite needs ffmpeg for listening. The original short fixture commands retain their Node + curl requirements. Cached audio works offline; the generated `index.html` shows the last suite prepared. This is broader coverage, not a balanced industry benchmark: most sectors currently have one recording, and Chinese real business meetings and verbatim Japanese meeting references remain gaps.

## Run recognition

ASR evaluation additionally needs Rust and the same [Tauri system dependencies](https://v2.tauri.app/start/prerequisites/) as desktop development (including `libasound2-dev` on Ubuntu). It downloads the required models automatically, without starting a GUI or opening a microphone:

```sh
pnpm eval:asr                                      # 17 quick cases, fixed SenseVoice baseline
pnpm eval:asr --policy dynamic                     # paired static/dynamic runs + derived stress controls
pnpm eval:asr --suite full                         # original 30 regression samples
pnpm eval:asr --model whisper-base                 # multilingual Whisper base int8 CPU baseline
pnpm eval:asr --denoise gtcrn                       # same inputs, with offline GTCRN denoising
pnpm eval:asr --suite full --model whisper-base --denoise gtcrn
pnpm eval:asr --vad on                            # production segmentation (default)
pnpm eval:asr --vad off                           # no VAD; fixed 10-second windows, including silence
pnpm eval:asr --only ja-fleurs-00,en-ami-00          # debug individual quick cases
pnpm eval:asr --model whisper-turbo                 # larger multilingual control
pnpm eval:asr --model reazonspeech                  # Japanese specialist + silence
pnpm eval:asr:sweep                                # all 19 candidate configurations, quick corpus
pnpm eval:asr:sweep --suite full                   # all candidates, full corpus
pnpm eval:asr:sweep --models sensevoice,qwen3-asr-0.6b,whisper-turbo
```

With `--vad on`, the runner calls the ignored Rust corpus test explicitly and uses the **same `voice::Engine`** as the desktop: Silero VAD, soft breaks, pre-roll, final recognition and voiceprint computation. Quiet input gets gradual, bounded gain (up to 8×) in the VAD's copy only; the recognizer and voiceprints receive the audio before that gain. Soft pauses are relative to the speaker's level. Native candidates change only the recognizer inside the test, with CPU inference and two ASR threads; they do not add a production setting. Model files have pinned SHA-256 checksums and candidate Hugging Face URLs pin upstream revisions.

The [candidate survey](candidates.md) records model families, language scope, exports and exclusions. Unsupported languages are omitted and listed in each report, never scored as successful empty transcripts. Silence remains in every configuration. SenseVoice, Whisper and Qwen use automatic language detection. Cohere requires an explicitly selected language, so `cohere-zh`, `cohere-ja` and `cohere-en` are labelled **fixed-language controls**, not a tested automatic router. References are never used as recognition prompts.

The three `wav2vec2-*` controls additionally need [uv](https://docs.astral.sh/uv/). Their isolated CPU Python environment is pinned in `ctc.py.lock`; it is not an application dependency. They reuse the desktop baseline's VAD intervals and replace every segment's recognition using the original audio. Their RTF is **recognizer-only**, excluding the separate segmentation pass, so it must not be compared directly with native pipeline RTF. Only static / VAD on / no GTCRN is currently supported for these controls.

`eval:asr:sweep` downloads candidates on demand, runs them sequentially, and writes individual reports plus `comparison.md` and `failures.json` under `eval/reports/asr-sweep/`. A broken download/model/inference is recorded and fails the sweep after the other candidates finish. The initial cold download is about 15 GB; models stay in ignored `.models/`. To compare reports from one compatible run independently, use `node eval/asr/compare.mjs REPORT_DIRECTORY`. It rejects changed corpus/engine/scorer versions, mismatched sample subsets and duplicate configurations. Accuracy is grouped by language; no global score mixes CER, WER and MER. Provisional Chinese numbers are explicitly separate.

`--vad off` bypasses VAD, its gain, soft breaks and pre-roll. It sends **every** sample, including silence, through the same recognizer and voiceprint extractor in fixed windows of at most 10 seconds. This bounds memory on long meetings and keeps the Whisper comparison within its input window, but may split words. It is a segmentation ablation, not a new application setting or a promise that whole-recording recognition is better. Reports and artifact names always include the VAD mode.

GTCRN runs before segmentation, so the comparison includes its effects on VAD and ASR. This is offline denoising, not an implemented real-time enhancement or overlapping-speaker separation feature. Original corpus files retain their noise, level and overlap. Only the separately labelled dynamic stress controls transform them in memory.

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

The quick suite contains 2 Chinese turns, 3 English turns, 3 Japanese utterances, all 8 mixed-language probes and silence. Full evaluation includes all original 30 fixtures; `--suite meetings` selects the separate long-meeting manifest. Selecting `--only` is recorded in the report and should not be compared with a full-suite aggregate.

Normalization is fixed in `score.mjs`: Unicode NFKC, lowercase, ignore punctuation/whitespace; English apostrophes and AMI acronym underscores are removed. Chinese/Japanese use Unicode characters, English uses words, mixed speech uses Han characters and Latin words. Numbers are not converted between spoken forms and digits; kanji/kana and simplified/traditional variants are not equated. These differences can affect scores. Error rates may exceed 100% when there are many insertions.

Rates are micro-averaged by reference units within a language, **not** averaged across languages. `ordered`, `provisional` and `unscored` samples are excluded from those totals; an absent score is shown as `—`, not zero errors. The original `unscored` meeting transcripts are not scored; long-suite `ordered` scores are explicitly diagnostic, never formal reference totals. Static controls have no absolute CER/WER merge threshold. Dynamic runs additionally enforce no added edits on the reference-scored paired cases.

Each run writes `input.json`, `hypotheses.json`, `report.json` and `report.md` under `eval/reports/asr/`, including corpus/version/hash, model hashes, engine hash, Git commit, working-tree status, VAD mode, input RMS/peak level, detected VAD segments/seconds, final segment count, hypotheses, reference text, CER/WER/MER, keyword recall and RTF. On GitHub, the Markdown also appears in the job summary. Input level is measured before denoising; VAD duration includes segment padding. These help distinguish low-level input, missed detection and empty recognition.

The production pipeline (SenseVoice, no GTCRN, VAD on) now fails on **any** empty speech transcript or speech hallucinated on silence, even when other cases succeed. Other combinations are experimental controls: the same checks are marked PASS/FAIL prominently in their reports and emit CI warnings, but do not block on expected ablation failures such as silence hallucinations without VAD. Download, checksum, configuration, inference and missing-result errors still fail **every** configuration. Native ONNX Runtime error markers and caught decoder exceptions are saved in `inference.stderr.log` and also fail every model, even if the decoder returns an empty segment and the rest of the meeting transcribes. The comparison labels runtime status separately; a process abort has no score and must not be interpreted as a pass. Static CER/WER remain measured without an absolute accuracy threshold.

The [initial four-way run](baseline.md) records the first full-corpus results and their limits.

## Dynamic policy and revisions

The desktop enables this first policy by default. `--policy static` (the eval default) disables optional retries; `--policy dynamic` runs **both** policies on identical input with the same model and VAD. It requires VAD on and no external GTCRN preprocessing. Gold text is never passed to the Rust engine. The report verifies that the initial transcriptions and boundaries match before comparing the final results.

- Capture retains at most **30 seconds per source** of 16 kHz mono audio before mixing, with source names and sample-relative start/end times. Continuous native VAD segments are flushed within that bound even when application soft cuts have already emitted text. These are mixer-aligned times, not hardware-clock synchronization. Recordings remain in memory and are discarded on stop/reset; meeting JSON stores quality signals and text provenance, not PCM.
- The primary route is unchanged: bounded VAD-only gain → original audio ASR → original audio CAM++. Each segment has a capture-scoped ID and revision. RMS, peak, clipped-sample fraction and per-source RMS are recorded; none is a speech-confidence score.
- Only VAD-detected, quiet segments (RMS 0.0005–0.006) or clipped mixtures with an unclipped source over four times stronger than the other source are eligible. A second pass either levels the same samples (at most 8×, peak at most 0.95), or uses that dominant original source. This is not a learned denoiser, echo canceller or speaker separator.
- At most one retry per segment, **12 seconds per retry**, two queued retries, and 30 seconds of audio age. Initial results are published first. The same worker/model runs retries between utterances; a native decode already running cannot be interrupted, so a retry can delay subsequent processing by its inference time. Stop/exit cancels queued optional work. Offline eval drains it to measure completed decisions.
- An empty transcript can recover; a nonempty result is replaced only by a bounded continuation of its original text (at least four characters, with complete Latin-word boundaries). Conflicting alternatives remain in the trace without replacing speech. This conservative heuristic is not proof that an extension is correct: paired reference samples fail evaluation on any added edit, and silence/noise controls must remain empty.
- The frontend updates the existing transcript by ID, keeps its speaker/page, discards superseded in-flight board results, and sends already-drawn revisions to the board agent as corrections. It does not replay the whole board. Agent corrections can still be wrong; the separate `revisions` text-to-board set checks node identity, unrelated content and one history entry per segment.

Dynamic quick evaluation adds 13 derived cases to the 17 originals: quiet (0.15×), deterministic additive noise, and dual-source same-speech echo for one short sample per language, plus noise-only silence. Full evaluation uses 30 originals plus the same controls. The long suite uses its nine unchanged recordings plus silence and a noise-only silence control; it does not multiply ten-minute recordings into synthetic variants or require a retry when no real segment qualifies. These are reproducible stress tests, **not new real recordings or overlapping-speaker separation ground truth**. Their scores are separate from the original language aggregates. Trace/report fields include trigger, action, original/candidate, accepted/retained/skipped decision, retry count/time, paired error delta and extra wall time. Disagreement can correctly result in zero accepted revisions; triggering alone is not an accuracy improvement.

```sh
pnpm eval:asr --policy dynamic --suite full
# The revisions set ran on the earlier app; the GPUI app has no transcript revisions yet (#150).
```

See the [first dynamic run](dynamic.md) for measured results and explicit limits.

## One workflow

[`.github/workflows/eval.yml`](../../.github/workflows/eval.yml) is the single testing workflow:

- Pushes to `main` and pull requests run type checks, unit tests, web build, four desktop builds with Rust tests, text-to-board/segment-revision evaluation, and quick SenseVoice ASR evaluation with **static VAD on/off plus paired dynamic** (three ASR jobs).
- Manual **Run workflow → full_asr** runs the full corpus with SenseVoice / Whisper base × raw / GTCRN × VAD on / off, plus paired dynamic SenseVoice (**nine jobs**).
- **asr_sweep** adds every candidate's static / VAD on / raw control (**26 jobs including the original ablations and dynamic control**). It uses the quick corpus unless `full_asr` is also selected. Every Monday at 02:30 UTC runs this expanded matrix on the original full corpus. Scheduled runs omit desktop packaging and the board model evaluation.
- **meeting_asr** selects only the long meeting suite and overrides the other ASR switches: SenseVoice, Paraformer, ReazonSpeech, Parakeet unified, Qwen3-ASR 0.6B and Whisper small, plus paired dynamic SenseVoice (**seven jobs**, VAD on). It also runs Thursdays at 02:30 UTC. Desktop packaging and board evaluation are omitted for this explicit ASR-only run. Long jobs have a 180-minute budget; ordinary push/short jobs keep 60 minutes.
- A final job collects matching reports into an **ASR language comparison** summary and artifact, even if another candidate failed. Failed candidate jobs remain failures; a partial comparison is not an all-model pass.
- Audio and models use separate caches keyed by their manifests. Restored files are always verified. Reports are uploaded as per-model artifacts for 30 days; audio/model caches are only accelerators, not the permanent source of the fixtures.

Data Releases use `asr-fixtures-*` / `asr-meetings-*` tags and are **not Latest**, so they do not trigger the `v*` software release workflow or replace the application's update manifest. Pages deployment and software releases remain separate workflows.
