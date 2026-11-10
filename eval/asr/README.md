# Speech evaluation

Measures the app's own speech recognition, `crates/audio`, on fixed recordings in Chinese, Japanese and English. The tool is `crates/asr-eval`:

```sh
cargo build --release -p boardcast-asr-eval
target/release/asr-eval download . --suite quick           # fetch and verify the recordings into eval/asr/.data
target/release/asr-eval run . --suite quick                # the app's hearing: speech detection, routed recognizers
target/release/asr-eval run . --suite quick --engine sensevoice
target/release/asr-eval run . --suite quick --engine sensevoice --vad off
target/release/asr-eval run . --suite full --enhance gain  # each recording brought to the same loudness first
target/release/asr-eval run . --only ja-fleurs-00,en-ami-00
target/release/asr-eval compare eval/reports/asr/*/report.json
```

The speech models are read from `resources/voice`, where `boardcast-tools fetch voice` puts them for the packages too. `download` checks every recording, cached or not, against its size and SHA-256 and replaces a missing or damaged one; nothing is skipped quietly. It also writes `eval/asr/.data/index.html`, a page to hear each recording beside its reference: open it from the folder, it needs no server and no script. The audio comes from the fixed [asr-fixtures-v1 Release](https://github.com/HsiangNianian/boardcast/releases/tag/asr-fixtures-v1); Release files are never replaced in place, so a change means a new version and new hashes.

`corpus.json` is the manifest for downloads, listening and scoring; `meetings.json` holds the long meetings. Source IDs, original intervals, licence links and reference quality travel with each sample. See [LICENSES.md](LICENSES.md) before redistributing audio or transcripts.

## What is measured

| `--engine` | `--vad` | What it is |
|---|---|---|
| `routed` (default) | `on` | What the app does: speech detection, SenseVoice for every sentence, then Parakeet for English and ReazonSpeech for Japanese where their models are present, with voiceprints. Its speech checks fail the run. |
| `sensevoice` | `on` | SenseVoice alone with the same speech detection: shows what the routing adds. |
| `sensevoice` | `off` | Fixed 10-second windows, silence included, through SenseVoice alone. Windows can split words; a control, not something the app does. |

`--enhance gain` brings each recording to about -20 dBFS first, to see whether level alone explains a miss. Audio is fed faster than real time; RTF is processing time over audio duration and excludes loading, so it is not microphone-to-screen latency, and timings compare only on the same hardware.

## Long meetings across industries

```sh
target/release/asr-eval download . --suite meetings   # downloads, verifies and cuts the clips; needs ffmpeg
target/release/asr-eval run . --suite meetings
```

`meetings.json` adds **nine continuous excerpts of about ten minutes each**, plus silence. These are independent clips, never concatenated short utterances or synthetic speech. The suite is separate from `quick` and `full`, so the regression on every push stays small.

| Language | Recordings | Domains | Reference treatment |
| --- | --- | --- | --- |
| Chinese | 3 AISHELL-4 recordings | Property management, kindergarten education, factory safety | Human-recorded **scenario meetings**, not verified everyday business meetings. Official manual TextGrids; overlapping turns receive diagnostic ordered CER. |
| English | 4 Earnings-22 conference calls | Texas Instruments / semiconductors, Costco / retail, KKR / finance, Imperial Oil / energy | Actual public business calls. Publisher manual references with forced alignment; ordinary WER. |
| English | 1 AMI IB4001 excerpt | Research-space planning | Actual non-scenario meeting. Different interval of the existing session, not an independent new session. Diagnostic ordered WER for overlap. |
| Japanese | 1 Ministry of the Environment web committee | Public health / asbestos relief policy | Actual meeting on 2023-03-31. Official **edited minutes**, not a verbatim gold transcript; provisional CER only. |

Every entry records the original interval, recording ID, industry, meeting kind, source URLs/hashes, reference timing policy and transformations. Chinese uses microphone channel 0; AMI uses the official mixed headset; MP3 sources become mono 16 kHz PCM16. No denoising, gain or silence removal is baked into these clips. The Japanese clip boundary was located using a preliminary ASR pass; its text comes entirely from the official minutes. There has been no fresh human transcript audit.

Chinese and AMI overlap is preserved. `ordered` concatenates manual turns by start time for a diagnostic score; another valid ordering can add errors. It is **not** overlap-aware meeting WER/CER, diarization accuracy or a speaker-separation benchmark, and does not enter formal reference aggregates. `provisional` minutes are likewise separate. English Earnings-22 keeps unaligned lexical words, removes explicitly marked nonlexical tags, and uses the publisher's spoken-form normalization; literal WER here is not their `fstalign` equivalence score. Unintelligible words and spoken-number/digit differences can affect results.

The [asr-meetings-v2 data Release](https://github.com/HsiangNianian/boardcast/releases/tag/asr-meetings-v2) publishes the current manifest and notice; it reuses the unchanged three AISHELL-4 WAVs and AMI WAV from [v1](https://github.com/HsiangNianian/boardcast/releases/tag/asr-meetings-v1). Earnings-22's license explicitly covers text, so its audio is downloaded directly from the pinned upstream Git revision. Japanese audio is also fetched directly from the ministry, whose temporary publication may be withdrawn. A missing/changed source fails the download; it is never silently omitted. Neither source's audio is uploaded in our Release. Original and converted files are SHA-256 checked; local conversion uses output-side ffmpeg bitexact flags to omit encoder-version metadata (identical converted WAVs checked with ffmpeg 6.0 and 9.0.1). A decoder/version mismatch fails explicitly instead of comparing different audio unnoticed.

Only this suite needs ffmpeg. It is broader coverage, not a balanced industry benchmark: most sectors have one recording, and real Chinese business meetings and verbatim Japanese meeting references remain gaps.

## Corpus and scores

The full pack contains 30 WAVs, about 26 MiB or 14 minutes, including overlapping short and long excerpts of the same meetings. It is a small regression fixture, not a representative multilingual benchmark.

| Group | Samples | Reference treatment |
| --- | ---: | --- |
| Chinese AliMeeting | 1 long meeting + 2 short turns | Community-derived text remains provisional. Short-turn CER is shown separately; excluded from reference aggregates. The long overlapping meeting is transcript-only. |
| English AMI | 1 long meeting + 5 isolated turns | Official manual references. Ordinary WER only on isolated turns; the long overlapping meeting is transcript-only. |
| Japanese FLEURS | 12 separate utterances | Publisher test-set references; **human read speech, not Japanese meetings**. CER on each recording. |
| Chinese-English ASCEND | 8 short clips | Publisher references from one session. MER uses individual Han characters plus Latin word/number tokens. Technical-term recall is reported where annotated. |
| Silence | 1 generated control | Any final recognized text is flagged; it fails the production-pipeline run. Ablations retain the failure as a reported measurement. |

The quick suite contains 2 Chinese turns, 3 English turns, 3 Japanese utterances, all 8 mixed-language probes and silence. Full evaluation includes all original 30 fixtures; `--suite meetings` selects the separate long-meeting manifest. Selecting `--only` is recorded in the report and should not be compared with a full-suite aggregate.

Normalization is fixed in `crates/asr-eval/src/score.rs`: Unicode NFKC, lowercase, and punctuation, symbols and spaces ignored; English apostrophes and AMI acronym underscores are removed. Chinese and Japanese count characters (CER), English words (WER), and mixed speech Han characters plus Latin words (MER). Spoken numbers and digits are not equated, nor kanji and kana or simplified and traditional characters, and these can affect scores. Rates can exceed 100% with many insertions.

Rates are micro-averaged by reference units within a language, never across languages. `ordered`, `provisional` and `unscored` samples stay out of those totals, and a missing score shows as `—`, not zero errors.

Each run writes `report.json` and `report.md` under `eval/reports/asr/`, and on GitHub the Markdown also goes to the job summary: the corpus version and manifest hash, the commit, the configuration, each sample's input level before any enhancement, detected speech seconds and sentences, which recognizer wrote how many sentences, the hypothesis beside the reference, the error rate, keyword recall and RTF.

The configuration the app ships (`routed`, VAD on, no enhancement) fails on **any** recording where speech went unheard, or silence was heard as words, even when the others succeed. The controls report the same checks prominently and as CI warnings, without failing on the failures they are expected to have, such as words in silence without speech detection. A download, checksum, configuration or model loading error fails every configuration. No absolute error-rate threshold is enforced.

## In CI

The [Eval workflow](../../.github/workflows/eval.yml) runs three jobs, one per row of the table above, on the quick samples for each push and pull request, and an **ASR comparison** puts their language totals side by side. **Run workflow → full_asr**, and Mondays at 02:30 UTC, use the full corpus; **meeting_asr**, and Thursdays at 02:30 UTC, the long meetings, with a 180-minute budget.

## Earlier evaluations

Until #150 the evaluation ran the earlier Tauri app's voice engine, which this app does not use. Its candidate survey (Whisper, Qwen3-ASR, Paraformer, wav2vec2 and others), GTCRN denoising and dynamic review were retired with it; the GPUI app's recognizers are SenseVoice, Parakeet and ReazonSpeech. Their reports stay as a record: [the first four-way run](baseline.md), [the candidate survey](candidates.md), [the 19-configuration sweep](sweep-2026-11-02.md), [the long meetings](long-meetings-2026-11-02.md) and [dynamic review](dynamic.md). A new candidate is evaluated by adding it to `crates/audio` first. `crates/audio/examples/` keeps narrower experiments: `asr-compare`, `asr-bench`, `asr-japanese` and `speaker-eval`.

Your own voice can be recorded for a personal check with [own-voice/record.sh](own-voice/README.md).
