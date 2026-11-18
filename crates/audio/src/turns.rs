//! Who is speaking, as they speak: Nemotron-3-Diarization (Streaming Sortformer) through our sherpa-onnx fork,
//! for #292. It hears the input continuously and says, for every 80 ms, how likely each of its four speaker
//! slots is to be talking. A sentence the detector cut out is split where one person stops and another goes
//! on, so two people in one breath get two lines; each piece carries the slot it belongs to, which names a
//! piece too short for a voiceprint.
//!
//! Speech over speech cannot be taken apart, so a voice heard under another's never cuts that sentence.

use sherpa_onnx::{OnlineSpeakerDiarization, OnlineSpeakerDiarizationConfig};
use std::collections::VecDeque;
use std::path::Path;

/// Under `models`, as `fetch voice` and the packages put it.
pub const MODEL: &str = "speaker/nemotron-3-diarization.int8.onnx";

/// How long after a moment its speakers are known, as NVIDIA's settings put it: encoder frames per step and
/// frames looked ahead. A sentence is heard once this much has passed after it; the detector's own pause
/// already covers part of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Latency {
    /// 1.04 s, the most accurate.
    Slow,
    /// 0.64 s.
    Medium,
    /// 0.32 s, about three times the work of `Slow`.
    Fast,
}

impl Latency {
    fn frames(self) -> (i32, i32) {
        match self {
            Self::Slow => (9, 4),
            Self::Medium => (6, 2),
            Self::Fast => (3, 1),
        }
    }
}

/// A speaker slot counts as talking above this.
const ACTIVE: f32 = 0.5;
/// A run shorter than this, in frames of 80 ms, is a flicker and goes to its neighbour.
const MIN_RUN: usize = 3;

pub(crate) struct Turns {
    model: OnlineSpeakerDiarization,
    /// Each frame's speaker probabilities, from frame `first` on.
    frames: VecDeque<Vec<f32>>,
    first: usize,
    /// Samples per frame.
    shift: usize,
    /// The ear's sample this stream began at: the model counts from zero again after each restart.
    origin: usize,
    /// How many times the stream restarted. Slots after a restart may be other people, so they are
    /// numbered on from the ones before.
    generation: usize,
    width: usize,
}

impl Turns {
    pub(crate) fn load(models: &Path, latency: Latency) -> Option<Self> {
        let path = models.join(MODEL);
        if !path.is_file() {
            return None;
        }
        let (chunk_length, chunk_right_context) = latency.frames();
        let config = OnlineSpeakerDiarizationConfig {
            model: Some(path.to_string_lossy().into_owned()),
            num_threads: 2,
            chunk_length,
            chunk_right_context,
            ..Default::default()
        };
        let model = OnlineSpeakerDiarization::create(&config)?;
        let shift = (model.frame_shift() * model.sample_rate() as f32).round() as usize;
        let width = usize::try_from(model.num_speakers()).unwrap_or(4).max(1);
        Some(Self { model, frames: VecDeque::new(), first: 0, shift: shift.max(1), origin: 0, generation: 0, width })
    }

    pub(crate) fn accept(&mut self, samples: &[f32]) {
        self.model.accept_waveform(samples);
        self.take();
    }

    /// The input ended: the frames still held back come out.
    pub(crate) fn finish(&mut self) {
        self.model.input_finished();
        self.take();
    }

    /// Begin a new stream at the ear's sample `at`, once everything before it was heard.
    pub(crate) fn restart(&mut self, at: usize) {
        self.model.reset();
        self.frames.clear();
        self.first = 0;
        self.origin = at;
        self.generation += 1;
    }

    fn take(&mut self) {
        let got = self.model.frames();
        if got.is_empty() {
            return;
        }
        if self.frames.is_empty() {
            self.first = got.first_frame;
        }
        let width = got.num_speakers.max(1);
        self.frames.extend(got.probs.chunks(width).map(<[f32]>::to_vec));
    }

    /// Every sample before this has its speakers known.
    pub(crate) fn known_until(&self) -> usize {
        self.origin + (self.first + self.frames.len()) * self.shift
    }

    /// Frames ending before this sample are no longer needed.
    pub(crate) fn forget_before(&mut self, sample: usize) {
        let frame = sample.saturating_sub(self.origin) / self.shift;
        while self.first < frame && !self.frames.is_empty() {
            self.frames.pop_front();
            self.first += 1;
        }
    }

    /// Where the `len` samples from the ear's sample `start` change hands: each piece's first sample,
    /// counted from `start`, its length and its speaker slot. One piece with no slot when nobody was heard.
    pub(crate) fn split(&self, start: usize, len: usize) -> Vec<(usize, usize, Option<usize>)> {
        let numbered = |pieces: Vec<(usize, usize, Option<usize>)>| pieces.into_iter().map(|(a, n, slot)| (a, n, slot.map(|s| s + self.generation * self.width))).collect();
        let Some(start) = start.checked_sub(self.origin) else {
            return vec![(0, len, None)];
        };
        let from = start / self.shift;
        let to = (start + len).div_ceil(self.shift);
        let rows: Vec<&[f32]> = (from..to).filter_map(|f| f.checked_sub(self.first).and_then(|i| self.frames.get(i))).map(Vec::as_slice).collect();
        // Frames already forgotten or not yet known leave the sentence whole.
        if rows.len() != to - from {
            return numbered(vec![(0, len, main_speaker(&rows))]);
        }
        let pieces = changes(&rows);
        let edge = |frame: usize| ((from + frame) * self.shift).saturating_sub(start).min(len);
        numbered(pieces.iter().map(|&(a, b, who)| (edge(a), edge(b) - edge(a), who)).filter(|p| p.1 > 0).collect())
    }
}

/// The slot talking most over these frames.
fn main_speaker(rows: &[&[f32]]) -> Option<usize> {
    let width = rows.first()?.len();
    let talk: Vec<usize> = (0..width).map(|s| rows.iter().filter(|r| r[s] > ACTIVE).count()).collect();
    talk.iter().enumerate().filter(|(_, n)| **n > 0).max_by_key(|(_, n)| **n).map(|(s, _)| s)
}

/// The turns in a run of frames, as (first frame, end frame, slot). A change of speaker becomes a cut only
/// once the one before has stopped: someone heard under another's voice stays in that person's piece.
pub(crate) fn changes(rows: &[&[f32]]) -> Vec<(usize, usize, Option<usize>)> {
    let n = rows.len();
    let loudest = |r: &[f32]| r.iter().enumerate().filter(|(_, p)| **p > ACTIVE).max_by(|a, b| a.1.total_cmp(b.1)).map(|(s, _)| s);
    let mut label: Vec<Option<usize>> = rows.iter().map(|r| loudest(r)).collect();
    let Some(first) = label.iter().flatten().next().copied() else {
        return vec![(0, n, None)];
    };
    // Pauses belong to whoever spoke before them.
    let mut last = first;
    for l in &mut label {
        last = l.unwrap_or(last);
        *l = Some(last);
    }
    let label: Vec<usize> = label.into_iter().flatten().collect();

    let mut runs: Vec<(usize, usize, usize)> = vec![];
    for (i, &who) in label.iter().enumerate() {
        match runs.last_mut() {
            Some(run) if run.2 == who => run.1 = i + 1,
            _ => runs.push((i, i + 1, who)),
        }
    }
    // Flickers join the run before them, or the next when they come first.
    let mut kept: Vec<(usize, usize, usize)> = vec![];
    for run in runs {
        match kept.last_mut() {
            Some(prev) if run.1 - run.0 < MIN_RUN => prev.1 = run.1,
            Some(prev) if prev.2 == run.2 => prev.1 = run.1,
            _ => kept.push(run),
        }
    }
    if kept.len() > 1 && kept[0].1 - kept[0].0 < MIN_RUN {
        let head = kept.remove(0);
        kept[0].0 = head.0;
    }

    // At each change, the cut waits for the speaker before to stop; if they never do, the next run was
    // said under their voice and stays theirs.
    let mut pieces: Vec<(usize, usize, usize)> = vec![];
    for run in kept {
        let Some(prev) = pieces.last_mut() else {
            pieces.push(run);
            continue;
        };
        if prev.2 == run.2 {
            prev.1 = run.1;
            continue;
        }
        let still = (run.0..run.1).take_while(|&f| rows[f][prev.2] > ACTIVE).count();
        let cut = run.0 + still;
        if run.1 - cut < MIN_RUN {
            prev.1 = run.1;
        } else {
            prev.1 = cut;
            pieces.push((cut, run.1, run.2));
        }
    }
    pieces.into_iter().map(|(a, b, who)| (a, b, Some(who))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Frames from a pattern per slot: `#` talking, `.` not.
    fn frames(slots: &[&str]) -> Vec<Vec<f32>> {
        let n = slots[0].len();
        (0..n).map(|i| slots.iter().map(|s| if s.as_bytes()[i] == b'#' { 0.9 } else { 0.05 }).collect()).collect()
    }

    fn run(slots: &[&str]) -> Vec<(usize, usize, Option<usize>)> {
        let f = frames(slots);
        let rows: Vec<&[f32]> = f.iter().map(Vec::as_slice).collect();
        changes(&rows)
    }

    #[test]
    fn one_speaker_is_one_piece() {
        assert_eq!(run(&["##########", ".........."]), vec![(0, 10, Some(0))]);
    }

    #[test]
    fn nobody_heard_is_one_piece_without_a_speaker() {
        assert_eq!(run(&["......", "......"]), vec![(0, 6, None)]);
    }

    #[test]
    fn a_change_of_speaker_is_cut() {
        assert_eq!(run(&["######......", "......######"]), vec![(0, 6, Some(0)), (6, 12, Some(1))]);
    }

    #[test]
    fn a_short_answer_between_two_turns_gets_its_own_piece() {
        assert_eq!(
            run(&["######....######", "......####......"]),
            vec![(0, 6, Some(0)), (6, 10, Some(1)), (10, 16, Some(0))]
        );
    }

    #[test]
    fn a_voice_under_another_never_cuts_it() {
        // The second slot is louder for a moment, but the first never stops.
        let mut f = frames(&["################", "......####......"]);
        for row in &mut f[6..10] {
            row[1] = 0.95;
        }
        let rows: Vec<&[f32]> = f.iter().map(Vec::as_slice).collect();
        assert_eq!(changes(&rows), vec![(0, 16, Some(0))]);
    }

    #[test]
    fn the_cut_waits_for_the_one_before_to_stop() {
        // They overlap for two frames; the cut comes where the first stops.
        let mut f = frames(&["########......", "......########"]);
        for row in &mut f[6..8] {
            row[1] = 0.95;
        }
        let rows: Vec<&[f32]> = f.iter().map(Vec::as_slice).collect();
        assert_eq!(changes(&rows), vec![(0, 8, Some(0)), (8, 14, Some(1))]);
    }

    #[test]
    fn flickers_and_pauses_do_not_cut() {
        assert_eq!(run(&["####.#####..####", "....#..........."]), vec![(0, 16, Some(0))]);
    }
}
