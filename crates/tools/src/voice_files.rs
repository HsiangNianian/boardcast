//! The speech models shipped in every package, about 1.1 GB: SenseVoice, the Silero VAD, the CAM++
//! speaker model, Parakeet for English and ReazonSpeech for Japanese. CI caches them by this file's hash.
//! The speaker model is not on ModelScope, so it comes from Hugging Face or GitHub.

pub struct VoiceFile {
    /// Where it goes under `resources/`.
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
    /// A ModelScope path, reached without a proxy.
    pub modelscope: Option<&'static str>,
    /// A Hugging Face path, tried on hf-mirror first.
    pub huggingface: String,
    pub github: Option<&'static str>,
}

const PARAKEET: &str = "csukuangfj2/sherpa-onnx-nemo-parakeet-unified-en-0.6b-int8-non-streaming/resolve/8c3a10fb13408c7a7054f6898958bf1c64a8d6c7";
const REAZON: &str =
    "reazon-research/reazonspeech-k2-v2/resolve/291488c8151be24d7da4bf7af26e533fad96e407";

pub fn files() -> Vec<VoiceFile> {
    let pinned = |repo: &str, file: &str| format!("{repo}/{file}");
    vec![
        VoiceFile {
            name: "voice/sense-voice/model.int8.onnx",
            size: 239_233_841,
            sha256: "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51",
            modelscope: Some("pengzhendong/sherpa-onnx-sense-voice-zh-en-ja-ko-yue/resolve/master/model.int8.onnx"),
            huggingface: "csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/model.int8.onnx".into(),
            github: None,
        },
        VoiceFile {
            name: "voice/sense-voice/tokens.txt",
            size: 315_894,
            sha256: "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc",
            modelscope: Some("pengzhendong/sherpa-onnx-sense-voice-zh-en-ja-ko-yue/resolve/master/tokens.txt"),
            huggingface: "csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/tokens.txt".into(),
            github: None,
        },
        VoiceFile {
            name: "voice/silero_vad.onnx",
            size: 1_807_522,
            sha256: "a35ebf52fd3ce5f1469b2a36158dba761bc47b973ea3382b3186ca15b1f5af28",
            modelscope: Some("pengzhendong/silero-vad/resolve/master/v4/silero_vad.onnx"),
            huggingface: "csukuangfj/vad/resolve/main/silero_vad.onnx".into(),
            github: None,
        },
        VoiceFile {
            name: "voice/speaker/campplus_zh_en.onnx",
            size: 28_281_164,
            sha256: "aa3cfc16963a10586a9393f5035d6d6b57e98d358b347f80c2a30bf4f00ceba2",
            modelscope: None,
            huggingface: "csukuangfj/speaker-embedding-models/resolve/main/3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx".into(),
            github: Some("https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx"),
        },
        VoiceFile {
            name: "voice/parakeet-en/encoder.int8.onnx",
            size: 654_040_552,
            sha256: "6716910b7a0833997fec7a410494c995d70124001a0e9b66d6370d6aced577e0",
            modelscope: None,
            huggingface: pinned(PARAKEET, "encoder.int8.onnx"),
            github: None,
        },
        VoiceFile {
            name: "voice/parakeet-en/decoder.int8.onnx",
            size: 7_257_753,
            sha256: "a5e223392c90e75f8144cdb5eb95af7625db389e39edef2bd1a9c872b3298fe6",
            modelscope: None,
            huggingface: pinned(PARAKEET, "decoder.int8.onnx"),
            github: None,
        },
        VoiceFile {
            name: "voice/parakeet-en/joiner.int8.onnx",
            size: 1_735_860,
            sha256: "869f43f7d24595c55581ad3bf249a935fb8a71389fbdaa7504b9f46f93140f8a",
            modelscope: None,
            huggingface: pinned(PARAKEET, "joiner.int8.onnx"),
            github: None,
        },
        VoiceFile {
            name: "voice/parakeet-en/tokens.txt",
            size: 8_952,
            sha256: "dc0b4584ab2e4ddbf888425c076c61b736e7356a015250db7d307e6f1a8188ff",
            modelscope: None,
            huggingface: pinned(PARAKEET, "tokens.txt"),
            github: None,
        },
        VoiceFile {
            name: "voice/reazon-ja/encoder-epoch-99-avg-1.int8.onnx",
            size: 154_670_139,
            sha256: "2c7bd08a8a99f9ddd0d9e458456577b1f6279214e51426f114f9eced44c54e1d",
            modelscope: None,
            huggingface: pinned(REAZON, "encoder-epoch-99-avg-1.int8.onnx"),
            github: None,
        },
        VoiceFile {
            name: "voice/reazon-ja/decoder-epoch-99-avg-1.int8.onnx",
            size: 2_959_337,
            sha256: "8f0bff94d38797b03b762634ed03211a8e303d06cc4603cdd0cf4199d6eb1485",
            modelscope: None,
            huggingface: pinned(REAZON, "decoder-epoch-99-avg-1.int8.onnx"),
            github: None,
        },
        VoiceFile {
            name: "voice/reazon-ja/joiner-epoch-99-avg-1.int8.onnx",
            size: 2_696_970,
            sha256: "49cc7ea1d3d35a40a27442db5e89996da64bf0e683a903dce76e99e57a12e4de",
            modelscope: None,
            huggingface: pinned(REAZON, "joiner-epoch-99-avg-1.int8.onnx"),
            github: None,
        },
        VoiceFile {
            name: "voice/reazon-ja/tokens.txt",
            size: 45_754,
            sha256: "2c3ac659818a48a0c04010e0593bbc4d7c8a24a054340b01131499c05fd52def",
            modelscope: None,
            huggingface: pinned(REAZON, "tokens.txt"),
            github: None,
        },
    ]
}
