# Dynamic policy baseline · 2026-11-02

Command: `pnpm eval:asr --policy dynamic --suite full`. SenseVoice int8, VAD on, no GTCRN.

43 paired cases: 30 original corpus samples and 13 deterministic stress controls (86 inference runs). This is a small regression suite, not representative accuracy or real overlapping-speaker separation ground truth.

| Original reference subset | Static | Dynamic |
|---|---:|---:|
| ja (12 samples) | 12.01% | 12.01% |
| en (5 samples) | 5.56% | 5.56% |
| zh-en (8 samples) | 30.19% | 30.19% |

Chinese references remain provisional and are excluded from that table. Japanese is read speech. Long overlapping meetings remain unscored. All paired reference edit deltas were zero; silence and noise-only controls remained empty.

The policy ran **22 second passes**, accepted **0 revisions**, and spent **4214 ms** on extra ASR. Trigger counts, including budget skips: {"quiet":29,"mixed_clipping":3}. Candidate disagreements were retained for inspection. **This run demonstrates routing/budget/provenance behavior and no scored regression; it does not demonstrate improved recognition accuracy.** Timings are local processing costs, not microphone-to-screen latency.

The separate `pnpm eval --set revisions --repeat 3` run used local Qwen3.5-4B Q4_K_M: Chinese/Japanese/English cases passed all nine node-identity/history/unrelated-content checks. They are synthetic correction tests, separate from ASR accuracy. An initial prompt left stale Chinese/English values as notes; the final revision instruction explicitly replaces incorrect existing values.

Validation also passed 172 JavaScript tests, 29 Rust tests with actual voice models, type checking, web build, Clippy and workflow validation. Linux virtual-input lifecycle tests passed preload, three cached starts (64/67/64 ms), stop, unload and shutdown. This is not end-to-end desktop UI or cross-platform hardware proof.

Reproduction identifiers (tested working tree based on `b17b180346f7454f0e86d72e8f2524cd399ec239`):

- Corpus manifest: `8dd4f34612c1a0e441d163176ad06b09d3aa5e94d1df728d408574fb5131b493`
- Voice engine: `f349d556b0cf04049b0e01ebc4d1080fba490995a97a8213b95765c70e82644c`
- Policy: `912f30b8603f2aec29bef3c227fac51e3922c79fd653d5a6e1dc99ac1ef9a606`
- Rust harness: `67ed439c65b90c09004e06718827a40233e2ce8893ea6981fc7333485bc3cb13`
- Local report folder: `eval/reports/asr/2026-11-02T02-12-45-409Z-sensevoice-none-vad-on-dynamic-full` (ignored; CI publishes its own reports as artifacts).

Live learned denoising, overlapping-speaker separation, hardware-clock source alignment and automatic alternate-ASR selection are not implemented by this first policy.
