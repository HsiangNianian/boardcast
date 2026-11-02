# Long meetings across industries — 2026-11-02

Nine continuous excerpts, **90.14 minutes** of speech recordings, replace the short-utterance view with a longer meeting check. The suite contains **six actual business/public/research meetings and three recorded Chinese scenario meetings**. It is separate from the original quick/full regression pack.

The [Eval run](https://github.com/HsiangNianian/boardcast/actions/runs/41389246416) compares six candidate models with VAD on and no external denoising, plus paired static/dynamic SenseVoice. Paraformer reports native decoder exceptions; Reazon aborts. These remain **failed candidates**, not successful empty recognition or zero-error scores. The workflow is therefore not an all-model pass.

## Data and reproduction

| Language | Continuous excerpts | Domains | Reference status |
|---|---:|---|---|
| Chinese | 3 × 9:53–10:00 | Property management, kindergarten education, factory safety | AISHELL-4 **scenario meetings**, manual TextGrids; ordered overlap diagnostic only |
| English | 4 × 10:00–10:07 | Texas Instruments / semiconductors, Costco / retail, KKR / finance, Imperial Oil / energy | Actual Earnings-22 business calls; publisher manual reference, ordinary WER |
| English | 1 × 10:00 | AMI IB4001 research-space planning | Actual non-scenario meeting; ordered overlap diagnostic only |
| Japanese | 1 × 10:09 | Ministry of the Environment public-health / asbestos-relief committee | Actual web committee, 2023-03-31; official edited minutes, provisional CER |

Clips are continuous source intervals, not stitched utterances or synthetic speech. A separate two-second silence control is included. Source IDs, intervals, pinned revisions/hashes, transformations and reference text are in [`meetings.json`](meetings.json). AMI uses another interval of the existing IB4001 recording, not an independent new session.

```sh
pnpm eval:listen --suite meetings   # download + serve; open http://127.0.0.1:4175
pnpm eval:asr:sweep --suite meetings --models sensevoice,paraformer-zh-en,reazonspeech,parakeet-unified-en,qwen3-asr-0.6b,whisper-small
pnpm eval:asr --suite meetings --policy dynamic
```

Listening to this suite needs Node.js 24+, curl and ffmpeg; inference also needs the desktop Rust build dependencies. The generated `eval/asr/.data/index.html` supports offline playback with the corresponding reference. Prepared WAVs total about 165 MiB, plus about 128 MiB of cached upstream MP3s. The [v2 data Release](https://github.com/HsiangNianian/boardcast/releases/tag/asr-meetings-v2) publishes the manifest and notice; its four redistributable WAVs come from the unchanged [v1 assets](https://github.com/HsiangNianian/boardcast/releases/tag/asr-meetings-v1). Other audio is fetched directly from the pinned publisher/source rather than mirrored. See [licenses and provenance](LICENSES.md).

The earlier v1 CI attempt stopped before inference because ffmpeg versions wrote different encoder metadata. V2 strips that metadata; conversions from all five upstream MP3s were byte-identical with ffmpeg 6.0 and 9.0.1. The tables below use only the v2 run, not a mixture of failed preparation and successful inference.

## English business calls: reference-scored WER

All rows use the same **four calls / 7,019 reference words**. Lower is better. A runtime PASS validates execution and speech/silence checks, not transcription correctness. These are literal normalized word errors: fillers are retained, and digits versus spoken numbers are not treated as equivalent. This is not the publisher's `fstalign` equivalence score or a percentage of correctly understood meaning.

| Model | WER | Errors / words | Pipeline RTF | Native runtime |
|---|---:|---:|---:|---|
| SenseVoice | 18.49% | 1298 / 7019 | 0.054 | PASS |
| Parakeet Unified EN | 13.79% | 968 / 7019 | 0.110 | PASS |
| Qwen3-ASR 0.6B | 14.18% | 995 / 7019 | 0.183 | PASS |
| Whisper small | 18.32% | 1286 / 7019 | 0.411 | PASS |
| Paraformer zh/en † | 24.66% | 1731 / 7019 | 0.055 | **FAIL** |

† Paraformer's score includes output after native exceptions returned empty segment results. It is diagnostic evidence of this failing integration, not a candidate to promote on the strength of its error rate.

Parakeet and Qwen both improve on SenseVoice on these four calls; their aggregate difference is only 27 word edits. Qwen leads their pairwise comparison on Texas Instruments and Costco; Parakeet leads on KKR and Imperial Oil. This does not establish a stable industry or language router. Each candidate ran on a separate GitHub `ubuntu-24.04` runner with two ASR threads; timings are observations, not a controlled same-host speed ranking. RTF includes the desktop VAD, segmentation and voiceprints, excludes model loading, and is not microphone-to-screen latency.

## Chinese, Japanese and overlapping English: diagnostic only

Chinese/AMI turns overlap and are concatenated by start time; another valid transcription order can add errors. Japanese text is edited official minutes, not verbatim speech. **None of these columns enters the reference-scored WER above.** There is no formal Chinese or Japanese gold aggregate in this long suite.

| Model | Chinese ordered CER, 6,710 chars | AMI ordered WER, 1,555 words | Japanese provisional CER, 2,912 chars |
|---|---:|---:|---:|
| SenseVoice | 13.95% | 35.18% | 26.96% |
| Parakeet Unified EN | — | 32.86% | — |
| Qwen3-ASR 0.6B | 14.32% | 30.29% | 33.59% |
| Whisper small | 42.46% | 36.08% | 34.96% |
| Paraformer zh/en † | 13.55% | 45.72% | — |
| ReazonSpeech | — | — | **No score: native abort** |

A dash is unsupported or unavailable, never zero errors. Reazon's Japanese specialist model did not complete. Current data does not justify replacing the production model for Chinese or Japanese.

The fixed scorer does not equate simplified/traditional Chinese or Japanese kanji/kana variants. Whisper mixes simplified and traditional output. As a separate local diagnostic, converting both Chinese references and hypotheses with the installed OpenCC `1.3.2.dirty` / `t2s.json` reduces Whisper's ordered CER from 42.46% to **36.81% (2,470 / 6,710)**; SenseVoice and Qwen stay at 13.95% and 14.32%. This post-hoc check is not the CI metric and does not change the scorer or remove the overlap caveat.

## Per recording / industry

All entries below are static, VAD on, no external denoising. The reference policy is part of the result; do not average different policies or languages together.

| Recording / domain | Policy / metric | SenseVoice | Parakeet EN | Qwen 0.6B | Whisper small | Paraformer † |
|---|---|---:|---:|---:|---:|---:|
| zh-property-meeting | ordered / CER | 16.65% | — | 17.52% | 38.87% | 17.86% |
| zh-education-meeting | ordered / CER | 16.05% | — | 17.97% | 48.79% | 15.32% |
| zh-manufacturing-meeting | ordered / CER | 9.02% | — | 7.35% | 39.95% | 7.26% |
| en-semiconductor-call | reference / WER | 26.68% | 24.02% | 22.04% | 25.69% | 31.11% |
| en-retail-call | reference / WER | 13.98% | 11.84% | 9.70% | 13.98% | 26.43% |
| en-finance-call | reference / WER | 13.95% | 10.98% | 11.20% | 14.34% | 27.14% |
| en-energy-call | reference / WER | 19.00% | 6.29% | 13.12% | 19.00% | 10.94% |
| en-research-meeting | ordered / WER | 35.18% | 32.86% | 30.29% | 36.08% | 45.72% |
| ja-health-committee | provisional / CER | 26.96% | — | 33.59% | 34.96% | — |

## Paired dynamic policy

The dynamic job runs its own static baseline before each dynamic pass, using the same input and checking that initial transcriptions/boundaries match. Use those pairs to measure policy gain, not the separately scheduled static job: a few characters differ between independent jobs even though their English aggregate happens to match.

CI recorded **46 retries, 0 accepted revisions and zero added/removed errors in every scored pair**. Retry inference totaled 3.057 seconds over the 90-minute corpus. Both silence and noise-only silence remained empty. Thus this run shows **no measured accuracy gain from the current dynamic policy**. A separate local v2 paired run also recorded 46 / 0, with unchanged scores and 3.473 seconds of retry inference.

The long suite adds only noise-only silence to the nine unchanged recordings and original silence; it does not generate quieter/noisier copies of every ten-minute meeting. All retry triggers here were quiet segments. This is evidence about the current conservative level-retry policy, not learned denoising, overlapping-speaker separation or an ASR/agent router.

## Runtime failures found by longer recordings

- **Paraformer:** 18 caught native decoder exceptions, represented by 54 error markers in stderr. ONNX `ConstantOfShape` / `Loop` failed with invalid tensor size, then returned empty results. The eval now fails on these markers even when the whole-recording transcript exists; its logs and diagnostic scores are retained.
- **ReazonSpeech:** ONNX convolution rejected shape `{1,39}` and the native exception aborted the process (`SIGABRT`). A local diagnostic reproduced the failure immediately after a **992-sample / 62 ms** ASR input, a short residual from meeting segmentation. The pinned export/adapter combination needs short-input handling before promotion; this failure gives no valid Japanese CER or general verdict on Reazon's accuracy.
- **SenseVoice residual text:** the static baseline emitted `<|Speech|>` on a 62 ms Japanese residual at 444.128–444.190 seconds. Whole-recording speech/silence checks passed, but this is still an incorrect segment output; no cleanup was applied before scoring.
- The comparison retains runtime FAIL labels and lists attempted runs with no scored report. A local sweep continues other candidates, then exits with failure rather than silently dropping failed models. Download/checksum/missing-result errors likewise remain failures.

No production recognition, routing or retry rule was changed to improve these benchmark scores. The debugging instrumentation was removed.

## Run identity and limits

- Measured commit: `a5deea5fb9c45db8d91202aba135611e78f1e51f`; CI working tree clean.
- Corpus: `asr-meetings-v2`; manifest SHA-256 `47e6610e34b2eb41c4add5757d3611f1dc5583281009f62130991b66acd05872`.
- Engine SHA-256: `f349d556b0cf04049b0e01ebc4d1080fba490995a97a8213b95765c70e82644c`.
- Policy SHA-256: `912f30b8603f2aec29bef3c227fac51e3922c79fd653d5a6e1dc99ac1ef9a606`.
- Rust harness SHA-256: `aad83dc611f11d75ad0772b4ab1c7b45e72fdd5ff72dc1ca2e3c8c7666a3ff8c`.
- Scorer SHA-256: `ea9d864e30e26ceae724fec68cdf62f54d250a984e4ed49d2aee83e6a49cbe83`.
- Runtime: sherpa-onnx 2.0.1, CPU, two recognizer threads; exact exported-model checksums are in each report and [`models.json`](models.json). These test the pinned exports and current desktop segmentation, not every upstream runtime/model configuration.
- Per-model `input.json`, `hypotheses.json`, scores, reference/hypothesis text and stderr are in the [Eval artifacts](https://github.com/HsiangNianian/boardcast/actions/runs/41389246416), retained for 30 days. An aborted candidate has input/logs but no scored hypothesis report.
- Type checks, unit tests and web build passed in that run; the final local reporting changes passed 181 unit tests. Two native candidate failures remain explicitly recorded.

Most domains have one recording. Chinese is recorded scenario speech rather than independently verified everyday business audio; Japanese minutes need a fresh human verbatim audit. The Japanese endpoint was located by a preliminary ASR pass, but its reference text comes entirely from the official minutes, never model-generated gold. There is no new long Chinese-English code-switching case; the existing short ASCEND controls remain. No new human listening audit, physical-microphone test, speaker-separation accuracy test or live UI latency measurement was performed. Larger Whisper/Qwen variants and the other short-sweep candidates have not yet been rerun on this long suite.
