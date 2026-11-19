//! Drafts: what is being said, shown while it is said (#291). A sentence's own text still comes when it ends,
//! from the recognizers that were measured for it; a draft only lets the transcript move with the speaker.
//!
//! Two ways to draft:
//! - [`DraftWith::Reread`] reads the speech so far again with a bundled recognizer every half second.
//!   SenseVoice takes tens of milliseconds a sentence, so it costs no download and covers every language.
//! - [`DraftWith::Stream`] feeds a streaming recognizer as audio comes, such as sherpa-onnx's Chinese
//!   streaming zipformer (161 MB), which never takes back what it showed.
//! - [`DraftWith::ByLanguage`] is the app's: the streaming model for Chinese, reading again for English
//!   and Japanese, as no streaming model under 200 MB drafts those well (measured with `asr-eval drafts`).

use sherpa_onnx::{OnlineRecognizer, OnlineRecognizerConfig, OnlineStream};
use std::path::{Path, PathBuf};

/// How drafts are made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DraftWith {
    /// Read the speech so far again, with a recognizer already loaded for whole sentences.
    Reread,
    /// A streaming model in this folder: a transducer (`encoder*.onnx`, `decoder*.onnx`, `joiner*.onnx`) or a
    /// Paraformer (`encoder*.onnx`, `decoder*.onnx`), with `tokens.txt`.
    Stream(PathBuf),
    /// The streaming model in this folder for Chinese, and reading again for every other language: the
    /// language chosen in Settings, or in automatic mode the language of the last sentence heard.
    ByLanguage(PathBuf),
}

/// What is being said, as far as it has been heard.
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub text: String,
    /// Which stretch of speech it belongs to: drafts of one share it, and the sentences heard from that
    /// stretch carry it too, as [`crate::Sentence::utterance`].
    pub utterance: usize,
    /// When the speech began and when this draft was made, in seconds of input.
    pub spoken_from: f64,
    pub at: f64,
}

/// The streaming model for Chinese drafts, under the voice models' folder.
pub const MODEL: &str = "draft-zh";

/// Rereading waits for this much more speech, in samples, before reading again: a third of a second.
pub(crate) const REREAD_STEP: usize = crate::SAMPLE_RATE as usize / 3;

pub(crate) struct Stream {
    recognizer: OnlineRecognizer,
    stream: OnlineStream,
    utterance: usize,
    /// Samples of the utterance given to the stream.
    fed: usize,
}

/// The file in `dir` whose name starts with `stem`, preferring an int8 one.
fn file(dir: &Path, stem: &str) -> Option<String> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(stem) && n.ends_with(".onnx"))).collect();
    found.sort_by_key(|p| !p.to_string_lossy().contains("int8"));
    found.first().map(|p| p.to_string_lossy().into_owned())
}

impl Stream {
    pub(crate) fn load(dir: &Path) -> Option<Self> {
        let mut config = OnlineRecognizerConfig::default();
        // A transducer has a joiner; a streaming Paraformer has only an encoder and a decoder.
        match file(dir, "joiner") {
            Some(joiner) => {
                config.model_config.transducer.encoder = Some(file(dir, "encoder")?);
                config.model_config.transducer.decoder = Some(file(dir, "decoder")?);
                config.model_config.transducer.joiner = Some(joiner);
            }
            None => {
                config.model_config.paraformer.encoder = Some(file(dir, "encoder")?);
                config.model_config.paraformer.decoder = Some(file(dir, "decoder")?);
            }
        }
        config.model_config.tokens = Some(dir.join("tokens.txt").to_string_lossy().into_owned());
        config.model_config.num_threads = 1;
        config.decoding_method = Some("greedy_search".into());
        let recognizer = OnlineRecognizer::create(&config)?;
        let stream = recognizer.create_stream();
        Some(Self { recognizer, stream, utterance: usize::MAX, fed: 0 })
    }

    /// Hear the utterance's speech so far, which grows, and say what it says.
    pub(crate) fn hear(&mut self, utterance: usize, speech: &[f32]) -> String {
        if utterance != self.utterance {
            self.stream = self.recognizer.create_stream();
            self.utterance = utterance;
            self.fed = 0;
        }
        if speech.len() > self.fed {
            self.stream.accept_waveform(crate::SAMPLE_RATE, &speech[self.fed..]);
            self.fed = speech.len();
        }
        while self.recognizer.is_ready(&self.stream) {
            self.recognizer.decode(&self.stream);
        }
        self.recognizer.get_result(&self.stream).map(|r| r.text.trim().to_string()).unwrap_or_default()
    }
}
