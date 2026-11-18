//! Who is speaking, as they speak: Nemotron-3-Diarization (Streaming Sortformer) through our sherpa-onnx fork,
//! for #292. It hears the input continuously and says, for every 80 ms, how likely each of its speaker slots,
//! eight in this model, is to be talking. A sentence the detector cut out is split where one person stops and another goes
//! on, so two people in one breath get two lines; each piece carries the slot it belongs to, which names a
//! piece too short for a voiceprint.
//!
//! Speech over speech cannot be taken apart, so a voice heard under another's never cuts that sentence. Each
//! piece says instead who else was heard talking in it, for the transcript to show (#295).
//!
//! The model costs about half of real time on two threads, so it hears only speech: the ear gives it what
//! the detector calls speech, from the first window loud enough to be part of it, and a little after, which
//! the model needs to give out the frames at the end of a sentence. [`Timeline`] keeps where in the ear's
//! input each stretch it heard came from.
//!
//! It runs on a thread of its own. Should it take more than four fifths of real time on this machine, the
//! ear stops splitting and names speakers by voice alone, as before.

use sherpa_onnx::{OnlineSpeakerDiarization, OnlineSpeakerDiarizationConfig};
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::Instant;
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
    /// 1.76 s: steps twice as long as `Slow`'s, so the model runs half as often.
    Long,
}

impl Latency {
    fn frames(self) -> (i32, i32) {
        match self {
            Self::Slow => (9, 4),
            Self::Medium => (6, 2),
            Self::Fast => (3, 1),
            Self::Long => (18, 4),
        }
    }
}

/// A speaker slot counts as talking above this.
const ACTIVE: f32 = 0.5;
/// A run shorter than this, in frames of 80 ms, is a flicker and goes to its neighbour.
const MIN_RUN: usize = 3;
/// Someone talking under a piece for this many frames, 0.32 s, counts as speaking at the same time.
const MIN_ALSO: usize = 4;

/// A piece of a sentence, split where the speaker changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Piece {
    /// Its first sample, counted from the sentence's start, and its length.
    pub from: usize,
    pub len: usize,
    /// The slot speaking it, if anyone was heard.
    pub slot: Option<usize>,
    /// Other slots heard talking at the same time.
    pub also: Vec<usize>,
}

/// Where the stretches of input the model heard came from: the model's samples follow one another, the
/// ear's have gaps where it heard silence.
#[derive(Debug, Default)]
pub(crate) struct Timeline {
    /// Each stretch's first sample in the ear's input and in the model's, and its length.
    runs: VecDeque<(usize, usize, usize)>,
    /// Samples the model has heard.
    fed: usize,
}

impl Timeline {
    /// The model hears `len` more samples, from the ear's sample `at` on.
    pub(crate) fn push(&mut self, at: usize, len: usize) {
        match self.runs.back_mut() {
            Some(run) if run.0 + run.2 == at => run.2 += len,
            _ => self.runs.push_back((at, self.fed, len)),
        }
        self.fed += len;
    }

    /// The ear's sample after the last one the model heard.
    pub(crate) fn end(&self) -> Option<usize> {
        self.runs.back().map(|run| run.0 + run.2)
    }

    /// The model's sample for the ear's: in a gap, the first heard after it.
    pub(crate) fn to_model(&self, ear: usize) -> usize {
        for &(at, model, len) in &self.runs {
            if ear < at {
                return model;
            }
            if ear < at + len {
                return model + (ear - at);
            }
        }
        self.fed
    }

    /// The ear's sample for the model's; past what it heard, the ear's sample after it.
    pub(crate) fn to_ear(&self, model: usize) -> usize {
        for &(at, start, len) in &self.runs {
            if model < start + len {
                return at + model.saturating_sub(start);
            }
        }
        self.end().unwrap_or(0)
    }

    /// Stretches that ended before the model's sample `model` are no longer needed.
    pub(crate) fn forget_before(&mut self, model: usize) {
        while self.runs.len() > 1 && self.runs[0].1 + self.runs[0].2 <= model {
            self.runs.pop_front();
        }
    }
}

/// What the ear asks of the model's thread.
enum Ask {
    Hear(Vec<f32>),
    /// The input ended: give out the frames still held back.
    Finish,
    /// Start a new stream.
    Reset,
}

/// What the model's thread gives back for each [`Ask`].
struct Answer {
    /// Which stream it belongs to, counting resets.
    generation: usize,
    first_frame: usize,
    probs: Vec<f32>,
    /// Audio heard and the time it took, in seconds.
    heard: f64,
    took: f64,
}

/// The model runs on a thread of its own, so the recognizers never wait for it.
fn spawn(model: OnlineSpeakerDiarization) -> (Sender<Ask>, Receiver<Answer>) {
    let (ask, asked) = channel::<Ask>();
    let (answer, answers) = channel::<Answer>();
    let rate = f64::from(model.sample_rate());
    thread::Builder::new()
        .name("diarization".into())
        .spawn(move || {
            let mut model = model;
            let mut generation = 0;
            for ask in asked {
                let began = Instant::now();
                let heard = match ask {
                    Ask::Hear(samples) => {
                        model.accept_waveform(&samples);
                        samples.len() as f64 / rate
                    }
                    Ask::Finish => {
                        model.input_finished();
                        0.0
                    }
                    Ask::Reset => {
                        model.reset();
                        generation += 1;
                        0.0
                    }
                };
                let got = model.frames();
                let took = began.elapsed().as_secs_f64();
                if answer.send(Answer { generation, first_frame: got.first_frame, probs: got.probs, heard, took }).is_err() {
                    return;
                }
            }
        })
        .expect("a thread for the diarization model");
    (ask, answers)
}

/// The model is too slow for this machine when, over this much audio, it took more than `TOO_SLOW` of its
/// length: with the recognizers on the same processor, it would fall ever further behind.
const JUDGED_OVER: f64 = 30.0;
const TOO_SLOW: f64 = 0.8;

pub(crate) struct Turns {
    ask: Sender<Ask>,
    answers: Receiver<Answer>,
    /// Asks sent and answers taken: when they match, the model has heard everything it was given.
    asked: usize,
    answered: usize,
    timeline: Timeline,
    /// Each frame's speaker probabilities, from frame `first` on.
    frames: VecDeque<Vec<f32>>,
    first: usize,
    /// Samples per frame.
    shift: usize,
    /// How many times the stream restarted. Slots after a restart may be other people, so they are
    /// numbered on from the ones before.
    generation: usize,
    width: usize,
    /// Audio heard and time taken lately, in seconds, to tell whether this machine keeps up.
    pace: VecDeque<(f64, f64)>,
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
        let (ask, answers) = spawn(model);
        Some(Self { ask, answers, asked: 0, answered: 0, timeline: Timeline::default(), frames: VecDeque::new(), first: 0, shift: shift.max(1), generation: 0, width, pace: VecDeque::new() })
    }

    fn send(&mut self, ask: Ask) {
        if self.ask.send(ask).is_ok() {
            self.asked += 1;
        }
    }

    /// Hear `samples`, which begin at the ear's sample `at`.
    pub(crate) fn accept(&mut self, at: usize, samples: &[f32]) {
        self.timeline.push(at, samples.len());
        self.send(Ask::Hear(samples.to_vec()));
    }

    /// The ear's sample after the last one heard.
    pub(crate) fn heard_until(&self) -> Option<usize> {
        self.timeline.end()
    }

    /// The input ended: the frames still held back come out, and this waits for them.
    pub(crate) fn finish(&mut self) {
        self.send(Ask::Finish);
        self.take(true);
    }

    /// Begin a new stream, once everything heard so far was given out.
    pub(crate) fn restart(&mut self) {
        self.send(Ask::Reset);
        self.timeline = Timeline::default();
        self.frames.clear();
        self.first = 0;
        self.generation += 1;
    }

    /// Take the frames the model has given back; with `wait`, until it has heard everything asked.
    pub(crate) fn take(&mut self, wait: bool) {
        loop {
            let answer = if wait && self.answered < self.asked {
                match self.answers.recv() {
                    Ok(a) => a,
                    Err(_) => return,
                }
            } else {
                match self.answers.try_recv() {
                    Ok(a) => a,
                    Err(_) => return,
                }
            };
            self.answered += 1;
            if answer.heard > 0.0 {
                self.pace.push_back((answer.heard, answer.took));
                while self.pace.iter().map(|p| p.0).sum::<f64>() - self.pace.front().map_or(0.0, |p| p.0) > JUDGED_OVER {
                    self.pace.pop_front();
                }
            }
            if answer.generation != self.generation || answer.probs.is_empty() {
                continue;
            }
            if self.frames.is_empty() {
                self.first = answer.first_frame;
            }
            self.frames.extend(answer.probs.chunks(self.width).map(<[f32]>::to_vec));
        }
    }

    /// Whether the model has lately taken too long for the audio it heard.
    pub(crate) fn too_slow(&self) -> bool {
        let (heard, took) = self.pace.iter().fold((0.0, 0.0), |(h, t), p| (h + p.0, t + p.1));
        heard >= JUDGED_OVER && took > heard * TOO_SLOW
    }

    /// Every sample of the ear's before this has its speakers known, or was never to be heard. `now` is
    /// the ear's sample after the last it was given.
    pub(crate) fn known_until(&self, now: usize) -> usize {
        let known = (self.first + self.frames.len()) * self.shift;
        if known >= self.timeline.fed { now } else { self.timeline.to_ear(known) }
    }

    /// Frames ending before the ear's sample `sample` are no longer needed.
    pub(crate) fn forget_before(&mut self, sample: usize) {
        let model = self.timeline.to_model(sample);
        let frame = model / self.shift;
        while self.first < frame && !self.frames.is_empty() {
            self.frames.pop_front();
            self.first += 1;
        }
        self.timeline.forget_before(self.first * self.shift);
    }

    /// Where the `len` samples from the ear's sample `start` change hands. One piece with no slot when
    /// nobody was heard.
    pub(crate) fn split(&self, start: usize, len: usize) -> Vec<Piece> {
        let offset = self.generation * self.width;
        let numbered = |pieces: Vec<Piece>| pieces.into_iter().map(|p| Piece { slot: p.slot.map(|s| s + offset), also: p.also.iter().map(|s| s + offset).collect(), ..p }).collect();
        let whole = |rows: &[&[f32]]| {
            let slot = main_speaker(rows);
            Piece { from: 0, len, slot, also: also_speaking(rows, slot) }
        };
        let (ear_start, end) = (start, self.timeline.to_model(start + len));
        let start = self.timeline.to_model(start);
        if end <= start {
            return vec![whole(&[])];
        }
        let from = start / self.shift;
        let to = end.div_ceil(self.shift);
        let rows: Vec<&[f32]> = (from..to).filter_map(|f| f.checked_sub(self.first).and_then(|i| self.frames.get(i))).map(Vec::as_slice).collect();
        // Frames already forgotten or not yet known leave the sentence whole.
        if rows.len() != to - from {
            return numbered(vec![whole(&rows)]);
        }
        let pieces = changes(&rows);
        let edge = |frame: usize| self.timeline.to_ear((from + frame) * self.shift).saturating_sub(ear_start).min(len);
        numbered(
            pieces
                .iter()
                .map(|&(a, b, slot)| Piece { from: edge(a), len: edge(b) - edge(a), slot, also: also_speaking(&rows[a..b], slot) })
                .filter(|p| p.len > 0)
                .collect(),
        )
    }
}

/// The slot talking most over these frames.
fn main_speaker(rows: &[&[f32]]) -> Option<usize> {
    let width = rows.first()?.len();
    let talk: Vec<usize> = (0..width).map(|s| rows.iter().filter(|r| r[s] > ACTIVE).count()).collect();
    talk.iter().enumerate().filter(|(_, n)| **n > 0).max_by_key(|(_, n)| **n).map(|(s, _)| s)
}

/// The slots other than `slot` heard talking for `MIN_ALSO` frames or more of these.
fn also_speaking(rows: &[&[f32]], slot: Option<usize>) -> Vec<usize> {
    let width = rows.first().map_or(0, |r| r.len());
    (0..width).filter(|&s| Some(s) != slot && rows.iter().filter(|r| r[s] > ACTIVE).count() >= MIN_ALSO).collect()
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

    #[test]
    fn the_timeline_maps_across_the_silence_left_out() {
        let mut t = Timeline::default();
        t.push(1000, 500);
        t.push(1500, 500);
        t.push(5000, 1000);
        assert_eq!(t.runs.len(), 2);
        assert_eq!((t.to_model(1200), t.to_model(3000), t.to_model(5500), t.to_model(9000)), (200, 1000, 1500, 2000));
        assert_eq!((t.to_ear(200), t.to_ear(1000), t.to_ear(1999), t.to_ear(2000)), (1200, 5000, 5999, 6000));
        assert_eq!(t.end(), Some(6000));
        t.forget_before(1200);
        assert_eq!(t.runs.len(), 1);
        assert_eq!(t.to_ear(1500), 5500);
    }

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
    fn a_voice_under_another_is_named_as_also_speaking() {
        let mut f = frames(&["################", "......####......", "..#............."]);
        for row in &mut f[6..10] {
            row[1] = 0.95;
        }
        let rows: Vec<&[f32]> = f.iter().map(Vec::as_slice).collect();
        // The third slot's one frame is a flicker, not someone speaking.
        assert_eq!(also_speaking(&rows, Some(0)), vec![1]);
        assert_eq!(also_speaking(&rows[..6], Some(0)), Vec::<usize>::new());
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
