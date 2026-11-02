//! Bounded, reference-free second passes. Original audio never leaves the capture worker.
use super::{rms, Heard, SAMPLE_RATE};
use serde::Serialize;
use std::collections::VecDeque;

pub(super) const RETAIN: usize = SAMPLE_RATE as usize * 30;
pub(super) const RETRY_MAX: usize = SAMPLE_RATE as usize * 12;
pub(super) const RETRY_QUEUE: usize = 2;

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quality {
    pub rms: f32,
    pub peak: f32,
    pub clipped: f32,
    pub source_rms: Vec<f32>,
}

impl Quality {
    pub(super) fn measure(samples: &[f32], tracks: &[Vec<f32>]) -> Self {
        Self {
            rms: rms(samples),
            peak: samples.iter().fold(0.0_f32, |p, x| p.max(x.abs())),
            clipped: samples.iter().filter(|x| x.abs() >= 0.999).count() as f32
                / samples.len().max(1) as f32,
            source_rms: tracks.iter().map(|s| rms(s)).collect(),
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub reason: String,
    pub action: String,
    pub original: String,
    pub candidate: String,
    pub decision: String,
    pub asr_ms: u32,
}

pub(super) struct Retry {
    pub heard: Heard,
    pub samples: Vec<f32>,
    pub queued_at: usize,
}

/// Aligned capture time, before mixing; zero-filled missing sources stay identifiable.
#[derive(Default)]
pub(super) struct SourceAudio {
    pub names: Vec<String>,
    tracks: Vec<VecDeque<f32>>,
    end: usize,
}

impl SourceAudio {
    pub fn push(&mut self, tracks: &[Vec<f32>]) {
        let Some(first) = tracks.first() else { return };
        assert!(tracks.iter().all(|s| s.len() == first.len()));
        if self.tracks.is_empty() {
            self.tracks.resize_with(tracks.len(), VecDeque::new);
        }
        assert_eq!(self.tracks.len(), tracks.len());
        self.end += first.len();
        for (saved, samples) in self.tracks.iter_mut().zip(tracks) {
            saved.extend(samples);
            saved.drain(..saved.len().saturating_sub(RETAIN));
        }
    }

    pub fn clip(&self, start: usize, len: usize) -> Vec<Vec<f32>> {
        self.tracks
            .iter()
            .map(|s| {
                let base = self.end - s.len();
                if start < base || start + len > self.end {
                    return vec![];
                }
                s.iter().skip(start - base).take(len).copied().collect()
            })
            .collect()
    }
}

/// One alternative only: recover a dominant unclipped source, or level quiet speech.
pub(super) fn plan(
    samples: &[f32],
    tracks: &[Vec<f32>],
    q: &Quality,
    text: &str,
) -> Option<(String, String, Vec<f32>)> {
    if q.clipped > 0.01 && tracks.len() == 2 && tracks.iter().all(|s| s.len() == samples.len()) {
        let dominant = usize::from(q.source_rms[1] > q.source_rms[0]);
        if q.source_rms[dominant] > 4.0 * q.source_rms[1 - dominant]
            && tracks[dominant].iter().all(|x| x.abs() < 0.999)
        {
            return Some((
                "mixed_clipping".into(),
                format!("source_{dominant}"),
                tracks[dominant].clone(),
            ));
        }
    }
    if (0.0005..0.006).contains(&q.rms) && q.peak > 0.0 {
        let gain = (0.03 / q.rms).min(8.0).min(0.95 / q.peak);
        if gain >= 1.5 {
            return Some((
                if text.is_empty() {
                    "empty_quiet"
                } else {
                    "quiet"
                }
                .into(),
                "level".into(),
                samples.iter().map(|s| s * gain).collect(),
            ));
        }
    }
    None
}

/// No language-model guesses: only recover empty speech or a bounded extension of the same text.
pub(super) fn accept(original: &str, candidate: &str) -> bool {
    let key = |s: &str| {
        let mut units = vec![];
        let mut word = String::new();
        for c in s.chars().flat_map(char::to_lowercase) {
            if c.is_ascii_alphanumeric() {
                word.push(c);
            } else {
                if !word.is_empty() {
                    units.push(std::mem::take(&mut word));
                }
                if c.is_alphanumeric() {
                    units.push(c.to_string());
                }
            }
        }
        if !word.is_empty() {
            units.push(word);
        }
        units
    };
    let a = key(original);
    let b = key(candidate);
    if b.is_empty() || a == b {
        return false;
    }
    a.is_empty()
        || (a.iter().map(|s| s.chars().count()).sum::<usize>() >= 4
            && b.starts_with(&a)
            && b.iter().map(|s| s.chars().count()).sum::<usize>()
                <= a.iter().map(|s| s.chars().count()).sum::<usize>() * 2 + 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_sources_keep_sample_times_and_original_values() {
        let mut audio = SourceAudio::default();
        audio.push(&[vec![0.2; RETAIN], vec![0.8; RETAIN]]);
        audio.push(&[vec![0.3; 100], vec![0.9; 100]]);
        assert_eq!(audio.tracks[0].len(), RETAIN);
        assert!(audio.clip(0, 10)[0].is_empty());
        assert_eq!(
            audio.clip(RETAIN - 2, 4),
            vec![vec![0.2, 0.2, 0.3, 0.3], vec![0.8, 0.8, 0.9, 0.9]]
        );
    }

    #[test]
    fn quiet_retry_is_bounded_and_silence_is_never_amplified() {
        for samples in [vec![0.0; 1000], vec![0.1; 1000]] {
            assert!(plan(&samples, &[], &Quality::measure(&samples, &[]), "hello").is_none());
        }
        let samples = vec![0.002; 1000];
        let (_, action, retry) =
            plan(&samples, &[], &Quality::measure(&samples, &[]), "hello").unwrap();
        assert_eq!(action, "level");
        assert!(retry.iter().all(|x| *x <= 0.016));
        assert_eq!(samples[0], 0.002);
    }

    #[test]
    fn disagreements_are_not_semantically_guessed() {
        assert!(accept("", "開発を始めます"));
        assert!(accept("开始时间", "开始时间是九点五十。"));
        assert!(!accept("九点", "八点半"));
        assert!(!accept("GitHub push", "GitLab push"));
        assert!(!accept("he works", "she works"));
        assert!(!accept("we do", "we don't agree"));
        assert!(!accept("I agree", "No, I agree"));
        assert!(accept("hello world", "Hello world again."));
        assert!(!accept("测试", "谢谢观看，欢迎订阅"));
        assert!(!accept("Hello!", "hello."));
        assert!(!accept("", "..."));
    }
}
