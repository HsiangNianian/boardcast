# Development priorities — 2026-11-02

The current priority is the product: UI, UX, features and the complete interaction flows in PAD whiteboard mode and PC meeting mode. Broad model selection and optimization resume after those flows are reasonably complete. Existing Eval regression coverage stays in place.

The explicitly requested exception is an experimental Apple SpeechAnalyzer option in desktop speech model management. SenseVoice with dynamic review and VAD on remains the default; this exception does not select a new cross-platform default.

## Apple transcription experiment

In **Settings → Local models → Speech recognition**, check availability, download any missing Apple language assets, then select **Use this**. The language list defaults to **Auto**; an existing explicit language choice is preserved. Use the normal microphone button or shortcut to record. Stop recording before switching providers or languages. The preference persists between launches; warmup opens no audio devices and downloads nothing automatically.

**Auto** identifies Chinese, Japanese or English once per recording from its opening speech. It uses the bundled SenseVoice language metadata with Silero VAD, then releases those detector models before starting Apple's transcriber in the chosen locale. It retains and replays the original opening PCM exactly once and preserves the recording's timestamp offset. Quiet waiting retains only one second of pre-roll. This adds an opening delay; it is not continuous multilingual switching or a confidence-calibrated classifier. Very short or mixed speech can need a manual language choice; unsupported or inconclusive detection asks for one instead of silently guessing a fallback language. No text-language model can replace this initial audio probe before a transcript exists.

Auto requires an available Apple locale for each of the three languages, preferring already installed assets and the system locale. Its download button prepares only missing assets, sequentially, through Apple. A manual language only needs its own assets and bypasses SenseVoice entirely. Auto warmup loads only the speech/language detector; manual warmup prepares the native analyzer. Neither path loads CAM++ for Apple transcription.

This integration targets **macOS 26+ desktop**. It queries `SpeechTranscriber.isAvailable`, `supportedLocales` and `installedLocales`, and checks `AssetInventory.status` again when starting a new analyzer. An Apple device alone does not guarantee support. The web app, including Safari on an iPad, cannot call this native API. There is no cloud or legacy-recognizer fallback.

The system API and boardcast's integration have different support boundaries:

| Environment | Apple SpeechAnalyzer | boardcast integration |
| --- | --- | --- |
| macOS 26+ | Available on compatible hardware and languages | Included in the desktop app; detected at runtime |
| Earlier macOS | This API does not exist | Use SenseVoice on supported desktop installations |
| iPadOS 26+ | Available to native apps on compatible devices | An iPad native bridge and app integration are still needed |
| Earlier iPadOS | This API does not exist | No Apple speech integration in boardcast |
| Browser, including iPad Safari | Cannot directly call the native Speech framework | No SpeechAnalyzer integration |

Apple's older [`SFSpeechRecognizer`](https://developer.apple.com/documentation/speech/sfspeechrecognizer) could be integrated separately for earlier systems. Its on-device support varies by device and language; a local-only implementation must first check [`supportsOnDeviceRecognition`](https://developer.apple.com/documentation/speech/sfspeechrecognizer/supportsondevicerecognition) and explicitly require on-device recognition. It is not the same API/model as this experiment and is not currently wired into boardcast.

Rust retains microphone/device selection, mixing and 16 kHz PCM routing. On macOS 13+, the same native helper captures system audio with ScreenCaptureKit and feeds it into that shared path; both SenseVoice and Apple transcription accept system-only or microphone + system input. Capture starts only on an explicit recording request, requires Screen & System Audio Recording permission (Screen Recording on older macOS), and stops with recording or app exit. The app subscribes only to audio and does not save screen images. A bundled Swift helper converts to the system's supported audio format and supplies a continuous sequence to `SpeechAnalyzer` + `SpeechTranscriber`. Volatile text updates the live caption; only final text enters the transcript and board. Apple does its own segmentation. This route does not apply SenseVoice's VAD/retry policy and currently uses the selected speaker instead of CAM++ automatic speaker attribution. It does not report invented ASR timings or signal-quality metrics.

Audio is piped locally and is not saved by the helper. Normal stop closes input, allows up to five seconds for final results, then reaps the helper. Failure to finalize is reported. Exit cancels native work, including downloads and warmup, and the helper watches for an abruptly terminated parent. Language assets remain system-managed; switching back to SenseVoice does not delete shared Apple assets.

Build on macOS with **Xcode 26+**. `scripts/build-apple-speech.sh` is used by Cargo and Eval; Tauri packages the executable beside the app binary. Eval compiles it, checks PCM conversion including split reads and system-audio buffer validation, and queries real system availability without opening a microphone or downloading Apple models. Auto also runs on verified Chinese/Japanese/English fixtures in the macOS job and the baseline Linux ASR job, checking opening language, bounded silent buffering and exact PCM replay. These checks are not Apple accuracy benchmarks or evidence of successful live recording on the user's Mac.

Official API references: [SpeechTranscriber](https://developer.apple.com/documentation/speech/speechtranscriber), [SpeechAnalyzer](https://developer.apple.com/documentation/speech/speechanalyzer), [AssetInventory](https://developer.apple.com/documentation/speech/assetinventory).

Validation on 2026-11-02: [Eval run 41401038026](https://github.com/HsiangNianian/boardcast/actions/runs/41401038026) built the macOS app and passed its Rust tests, native PCM self-test and capability query. The virtual Mac returned `isApple: true`, `available: false`, `reason: hardware`; no Apple transcription accuracy or live microphone behavior was measured there. Locally, 181 JavaScript tests and 32 Rust tests passed, including the real-audio SenseVoice regressions. UI checks used simulated availability/download states in an isolated Linux browser. An actual supported Mac is still needed to validate Apple model installation, live transcription and latency.

## Evidence to retain before resuming model work

- [19-configuration short sweep](../eval/asr/sweep-2026-11-02.md): Qwen 1.7B's community ONNX output is anomalous; it does not establish the upstream model's accuracy ceiling. All major Whisper sizes, including large-v3 and turbo, already have short-test results.
- [Long meeting report](../eval/asr/long-meetings-2026-11-02.md): English business-call reference WER favors Parakeet Unified EN and Qwen 0.6B over the current SenseVoice run. Chinese overlapping speech uses an ordered diagnostic score; Japanese edited minutes are provisional, not a verified verbatim reference. These numbers cannot establish one winner across all three languages.
- Dynamic review made 46 retries and accepted none in the long paired run. It did not improve the measured recognition error. Its conservative continuation rule is not yet a general correction policy for a second model.
- ReazonSpeech's long-run crash followed a 992-sample tail (about 62 ms). Preserve useful short responses while checking merge/padding/minimum-input behavior; a runtime failure is not a valid accuracy result.

## Deferred experiments

1. Audit Qwen3-ASR 1.7B export/runtime correctness with identical PCM across an official implementation, an independent Apple implementation and current ONNX. Record raw output, prompts/tokenizer, feature lengths, padding/masks, EOS and generation limits before changing quantization.
2. Compare WhisperKit/Core ML large-v3-turbo with the current runtime on the same audio, separating explicit-language inputs from automatic-language inputs. Then compare each runtime's appropriate segmentation. Offline chunking is not native streaming.
3. Verify Fun-ASR-Nano's current official assets and add it as a Chinese/English/Japanese candidate if its deployment route fits. Do not conflate it with the separate multilingual checkpoint.
4. Repair ReazonSpeech's short-tail adapter and obtain human-checked Japanese verbatim references. Expand mixed-language and industry coverage before considering language routing.
5. Evaluate cross-model correction separately from signal-level retries: measure corrections, harmful changes and downstream board changes. Keep original audio ranges and transcript identities.
6. Measure Apple implementations on the actual target devices under concurrent whiteboard LLM load: first-text delay, final-text P95, memory, temperature and sustained slowdown. Linux CPU RTF is not microphone-to-screen latency.

The supplied external analysis proposes MLX Qwen, speech-swift, WhisperKit, Fun-ASR and Argmax Pro routes. These are research leads, not project-validated implementations or licensing promises. Recheck their official support, device requirements and offline/license behavior when the experiments resume. The target Mac's exact hardware and memory still need confirmation.
