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
- Model checksums are pinned in `scripts/fetch-voice.mjs` and `eval/asr/models.json`. Whisper is **base int8**, not large-v3/turbo. It is a lightweight comparison, not a proxy for stronger Whisper models or Qwen3-ASR.
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
