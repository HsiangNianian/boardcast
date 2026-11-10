# Initial ASR regression run — 2026-11-01

All four full-corpus combinations completed locally (30 samples each, 120 sample evaluations). These numbers describe this small fixed regression set; they are not a model leaderboard or a claim about general meeting quality.

| CPU model | Preprocessing | Japanese CER (12 read utterances) | English WER (5 isolated turns) | Chinese-English MER (8 probes) |
| --- | --- | ---: | ---: | ---: |
| sensevoice | none | 11.65% | 6.35% | 28.30% |
| sensevoice | gtcrn | 13.98% | 7.94% | 35.85% |
| whisper-base | none | 34.95% | 11.11% | 81.13% |
| whisper-base | gtcrn | 33.51% | 9.52% | 64.15% |

- Corpus: `asr-fixtures-v1`; manifest SHA-256 `8dd4f34612c1a0e441d163176ad06b09d3aa5e94d1df728d408574fb5131b493`.
- Runtime: sherpa-onnx 2.0.1; recognizer threads = 2; automatic language detection; the application's Silero VAD, pre-roll and soft sentence breaks are retained.
- Model checksums are pinned in `crates/tools/src/voice_files.rs` and `eval/asr/models.json`. Whisper is **base int8**, not large-v3/turbo. It is a lightweight comparison, not a proxy for stronger Whisper models or Qwen3-ASR.
- Counts: Japanese 558 reference characters; English 126 words; mixed speech 53 mixed units. Micro-averaged within each language; no combined multilingual average.
- Chinese provisional references and both long overlapping meetings are excluded from these aggregate rates. One of the two very quiet Chinese short turns produced no final transcript in the raw SenseVoice run; it remains in the fixture rather than being removed as an inconvenient case.
- The silence control produced no final text in all four combinations.
- GTCRN worsened SenseVoice's scores on these reference subsets and improved Whisper base's scores. This does not justify enabling denoising globally; recording conditions and models matter.
- No accuracy threshold is enforced. This file is a record of the initial run, not an automatically rewritten golden output. Performance timings are omitted because this was a shared development machine, not a controlled timing benchmark.

Reproduce each row with:

```sh
pnpm eval:asr --suite full --model sensevoice --denoise none
pnpm eval:asr --suite full --model sensevoice --denoise gtcrn
pnpm eval:asr --suite full --model whisper-base --denoise none
pnpm eval:asr --suite full --model whisper-base --denoise gtcrn
```

The runner writes per-sample reference/hypothesis text and errors to `eval/reports/asr/`; CI uploads them as artifacts and job summaries. Changing data, normalization, model files or segmentation requires a fresh comparison with the relevant versions recorded.

## Quiet input and VAD on/off — 2026-11-01

All eight full-corpus combinations completed locally (30 samples each, **240 sample evaluations**). This follows the initial run above at `f080526`; it uses the same audio files, references, model weights and scoring rules. The engine changed, so these rows supersede the initial production-pipeline baseline.

The empty Chinese case was a segmentation failure: the original waveform could be transcribed directly, but raw Silero VAD emitted no segment. The fix applies gradual gain capped at 8× only to quiet VAD input, preserves the original samples for ASR/voiceprints, and prevents the soft-pause floor from exceeding quiet speech itself. Normal-level input bypasses gain; silence does not increase it.

| Case | Original result | Result after the fix |
| --- | --- | --- |
| `zh-alimeeting-00` (−45.8 dBFS) | Only a 2.90-second segment of 11.39 seconds; provisional CER 77.27% | Speech across the meeting turn is retained; provisional CER 22.73% |
| `zh-alimeeting-01` (−46.3 dBFS) | No final text; provisional CER 100% | 10.71 seconds detected; provisional CER 8.62% |
| `ja-fleurs-05` (−59.4 dBFS) | Only the last phrase; CER 38.10% | More of the sentence retained; CER 19.05% |

Chinese references are still provisional and excluded from language aggregates. This fixes dropped speech, not every transcription error. The production pipeline's English WER changed from 6.35% to 5.56%, Japanese CER from 11.65% to 12.01%, and mixed MER from 28.30% to 30.19%. The latter two are small regressions (two Japanese characters and one mixed unit); no universal accuracy improvement is claimed.

| CPU model | Preprocessing | VAD | Japanese CER | English WER | Chinese-English MER | Text on silence |
| --- | --- | --- | ---: | ---: | ---: | --- |
| sensevoice | none | on | 12.01% | 5.56% | 30.19% | none |
| sensevoice | none | off | 10.93% | 7.14% | 35.85% | 그. |
| sensevoice | gtcrn | on | 12.01% | 7.94% | 35.85% | none |
| sensevoice | gtcrn | off | 10.75% | 8.73% | 33.96% | 그. |
| whisper-base | none | on | 36.74% | 10.32% | 79.25% | none |
| whisper-base | none | off | 29.03% | 11.11% | 75.47% | Ayn ym. |
| whisper-base | gtcrn | on | 37.28% | 9.52% | 64.15% | none |
| whisper-base | gtcrn | off | 31.36% | 9.52% | 71.70% | Ayn ym. |

VAD-off processes silence too, in fixed 10-second windows; those boundaries can split words. All four VAD-off controls hallucinated text on the silence fixture. All four VAD-on runs had text for every speech case and no text on silence. The production configuration enforces these checks; experimental controls show failed checks and CI warnings without hiding their measurements. Every configuration still fails on download, configuration, inference or missing-result errors.

Validation also includes 24 Rust tests with actual models (quiet speech, synthetic noise/hum, silence, reset, continuous speech and voiceprints), 167 JavaScript tests, type checking, web build, Clippy and workflow lint. Physical microphones, echo, overlapping-speaker separation and microphone-to-screen latency were not evaluated. Parallel local runs are not a controlled performance benchmark.

Reproduce VAD on/off with the same model and preprocessing:

```sh
pnpm eval:asr --suite full --model sensevoice --denoise none --vad on
pnpm eval:asr --suite full --model sensevoice --denoise none --vad off
```

Engine SHA-256: `bdca55e94fd6352743f817bcb4c1026b8376b48a64850af326873462dbc38536`. The corpus and model hashes remain pinned as above; per-run JSON records VAD mode, levels, segmentation, hypotheses and check outcomes.
