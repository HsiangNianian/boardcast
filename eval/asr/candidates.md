# ASR candidate survey

Surveyed the [Hugging Face ASR download ranking](https://huggingface.co/models?pipeline_tag=automatic-speech-recognition&sort=downloads), first 60 entries, on 2026-11-02. Downloads identify candidates, not accuracy. Format conversions of the same weights are not independent model families. The first runnable sweep covers **19 configurations**, with the same corpus and scoring rules. These are evaluation candidates; production remains SenseVoice with dynamic review and VAD on.

The [first measured sweep](sweep-2026-11-02.md) completed all 19 candidates and seven controls in Eval, with language-specific scores and raw transcript artifacts.

| Candidates | Corpus languages | Evaluation form | Source |
|---|---|---|---|
| SenseVoice | zh / ja / en / zh-en | Existing INT8 ONNX baseline, auto language | [FunAudioLLM](https://huggingface.co/FunAudioLLM/SenseVoiceSmall) |
| Whisper tiny / base / small / medium / large-v3 / turbo | zh / ja / en / zh-en | sherpa INT8 ONNX exports, auto language | [OpenAI](https://github.com/openai/whisper#available-models-and-languages) |
| Qwen3-ASR 0.6B / 1.7B | zh / ja / en / zh-en | INT8 ONNX exports, auto language | [Qwen](https://huggingface.co/Qwen/Qwen3-ASR-1.7B) |
| Paraformer bilingual | zh / en / zh-en | INT8 ONNX | [Export and upstream model link](https://huggingface.co/csukuangfj/sherpa-onnx-paraformer-bilingual-zh-en) |
| Parakeet TDT v3 | en | INT8 ONNX; its European language support does not include zh/ja | [NVIDIA](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3) |
| Parakeet Unified English | en | Non-streaming INT8 ONNX export | [Export](https://huggingface.co/csukuangfj2/sherpa-onnx-nemo-parakeet-unified-en-0.6b-int8-non-streaming) |
| ReazonSpeech k2 v2 | ja | Publisher's INT8 ONNX files, 159M Japanese specialist | [Reazon](https://huggingface.co/reazon-research/reazonspeech-k2-v2) |
| Cohere Transcribe | zh / ja / en, **three separate configurations** | INT8 ONNX, explicit language selection; mixed speech not claimed | [Cohere](https://huggingface.co/CohereLabs/cohere-transcribe-03-2026) |
| wav2vec2 XLSR Japanese / Chinese | ja / zh, respectively | FP32 Transformers CTC on desktop VAD intervals | [Japanese](https://huggingface.co/jonatasgrosman/wav2vec2-large-xlsr-53-japanese), [Chinese](https://huggingface.co/jonatasgrosman/wav2vec2-large-xlsr-53-chinese-zh-cn) |
| wav2vec2 base 960h | en | FP32 Transformers CTC on desktop VAD intervals | [Meta](https://huggingface.co/facebook/wav2vec2-base-960h) |

Exact export repositories, revisions, byte sizes and SHA-256 hashes are in [`models.json`](models.json). In particular, Qwen 1.7B uses a community export (`ilmina`); results establish the behavior of those exported weights and runtime, not equivalence with the upstream BF16 implementation. Quantization and decoding can change accuracy. Transducer/CTC controls use greedy decoding without an external language model. Native recognition uses the existing sherpa-onnx dependency, CPU and two threads. The Python controls pin their dependencies separately.

## Ranking entries not counted as new ASR candidates

- **WhisperKit/Core ML, faster-whisper, MLX and GGUF ports**: represented by the underlying model family, but their platform optimizations and numerical equivalence have not been benchmarked here. This sweep does not claim to test every runtime or quantization.
- **pyannote diarization and forced aligners**: solve speaker attribution or alignment, not independent audio-to-text recognition. They need different ground truth and metrics.
- **Language-specific models outside Chinese, Japanese and English**: outside this corpus. No invented score is assigned.
- **Nemotron 3.5 streaming and Voxtral Realtime**: retained as candidates for a streaming-runtime evaluation; not interchangeable with the current offline recognizer. Their buffering/latency and model-specific runtime need their own integration. [Nemotron export](https://huggingface.co/handy-computer/nemotron-3.5-asr-streaming-0.6b-gguf), [Voxtral](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602).
- **Apple SpeechTranscriber / SpeechAnalyzer**: a Mac-specific candidate, not measured on this Linux host. A future measurement must record hardware, OS version, supported/installed locales and chosen transcriber; system model assets can update. Do not conflate it with legacy `SFSpeechRecognizer`, assume every locale is installed, or silently use a cloud fallback. [Apple's API introduction](https://developer.apple.com/videos/play/wwdc2025/277/).

## Interpreting language-specific results

Compare the same original samples with the same VAD mode and preprocessing. Language-specialist rows only cover the languages they support; the benchmark's sample labels select their evaluation subset, **not a production language detector**. Cohere's fixed language is additional input and is shown explicitly. Accuracy of a future router must include incorrect language decisions, code-switching, uncertain short utterances, latency and model memory costs.

The corpus is a regression fixture, not a representative leaderboard: Chinese AliMeeting text is provisional, Japanese FLEURS is read speech, English AMI has only a few reference-scored turns, and mixed ASCEND probes come from one session. Never pick a production language route from one tiny aggregate. Keep model downloads and raw transcripts reproducible; enlarge and human-check the relevant holdout before promotion.
