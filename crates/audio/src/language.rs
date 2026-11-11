//! The former Tauri opening-language probe, using the current ear's VAD-segmented sentence.
use crate::SAMPLE_RATE;
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use sherpa_onnx_sys as sys;
use std::ffi::{CStr, CString};
use std::path::Path;

pub(crate) struct Probe(*const sys::OfflineRecognizer);
// SAFETY: The recognizer has one owner and is used serially, like sherpa-onnx's safe wrapper.
unsafe impl Send for Probe {}

fn locale(result: &Value) -> Result<&'static str> {
    let language = result["lang"].as_str().or_else(|| result["language"].as_str()).unwrap_or_default();
    let event = result["event"].as_str().unwrap_or_default();
    let language = language.trim_start_matches("<|").trim_end_matches("|>");
    let event = event.trim_start_matches("<|").trim_end_matches("|>");
    let speech = event.eq_ignore_ascii_case("speech") && result["text"].as_str().is_some_and(|s| !s.trim().is_empty());
    match (speech, language) {
        (true, "zh") => Ok("zh-CN"),
        (true, "ja") => Ok("ja-JP"),
        (true, "en") => Ok("en-US"),
        _ => anyhow::bail!("Apple Auto could not identify Chinese, Japanese or English speech. Choose a language for this source and start again."),
    }
}

impl Probe {
    pub(crate) fn load(models: &Path) -> Result<Self> {
        ensure!(models.join("sense-voice/model.int8.onnx").is_file() && models.join("sense-voice/tokens.txt").is_file(), "Apple Auto needs the multilingual speech package. Choose a language manually or install that package.");
        let model = CString::new(models.join("sense-voice/model.int8.onnx").to_string_lossy().as_bytes())?;
        let tokens = CString::new(models.join("sense-voice/tokens.txt").to_string_lossy().as_bytes())?;
        // SAFETY: The C config has only numbers and nullable pointers, as in the upstream example.
        let mut config: sys::OfflineRecognizerConfig = unsafe { std::mem::zeroed() };
        config.feat_config.sample_rate = SAMPLE_RATE;
        config.feat_config.feature_dim = 80;
        config.model_config.sense_voice.model = model.as_ptr();
        config.model_config.sense_voice.language = c"auto".as_ptr();
        config.model_config.tokens = tokens.as_ptr();
        config.model_config.num_threads = 2;
        config.model_config.provider = c"cpu".as_ptr();
        config.decoding_method = c"greedy_search".as_ptr();
        config.max_active_paths = 4;
        // SAFETY: The factory copies the config strings before returning.
        let ptr = unsafe { sys::SherpaOnnxCreateOfflineRecognizer(&config) };
        ensure!(!ptr.is_null(), "Apple Auto needs the bundled SenseVoice language detector");
        Ok(Self(ptr))
    }

    pub(crate) fn detect(&self, samples: &[f32]) -> Result<&'static str> {
        ensure!(!samples.is_empty() && samples.len() <= SAMPLE_RATE as usize * 5 && samples.iter().all(|s| s.is_finite()), "invalid audio for language detection");
        // SAFETY: Handles belong to this call; samples stay valid through synchronous decoding.
        let json = unsafe {
            let stream = sys::SherpaOnnxCreateOfflineStream(self.0);
            ensure!(!stream.is_null(), "could not create language detection stream");
            sys::SherpaOnnxAcceptWaveformOffline(stream, SAMPLE_RATE, samples.as_ptr(), samples.len() as i32);
            sys::SherpaOnnxDecodeOfflineStream(self.0, stream);
            let result = sys::SherpaOnnxGetOfflineStreamResultAsJson(stream);
            let json = (!result.is_null()).then(|| CStr::from_ptr(result).to_string_lossy().into_owned());
            if !result.is_null() { sys::SherpaOnnxDestroyOfflineStreamResultJson(result); }
            sys::SherpaOnnxDestroyOfflineStream(stream);
            json.context("language detection returned no result")?
        };
        locale(&serde_json::from_str(&json).context("invalid language detection result")?)
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        // SAFETY: This is the unique owning handle created above.
        unsafe { sys::SherpaOnnxDestroyOfflineRecognizer(self.0) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detection_requires_spoken_language_metadata_not_transcript_letters() {
        for (lang, expected) in [("zh", "zh-CN"), ("<|ja|>", "ja-JP"), ("en", "en-US")] {
            assert_eq!(locale(&json!({"lang":lang,"event":"<|Speech|>","text":"API"})).unwrap(), expected);
        }
        assert_eq!(locale(&json!({"language":"en","event":"Speech","text":"hello"})).unwrap(), "en-US");
        for value in [json!({"text":"hello"}), json!({"lang":"en","event":"Music","text":"hello"}),
            json!({"lang":"ko","event":"Speech","text":"hello"}), json!({"lang":"en","event":"Speech","text":" "})] {
            assert!(locale(&value).is_err());
        }
    }
}
