//! How much later each line arrives when sentences wait for the diarization model, and what the model costs:
//! a recording is heard without it and at each latency, and when each line arrives is measured in input
//! time, so the result does not depend on how fast this machine is. Lines given out by the final flush are
//! left out, as nothing waited for them. Two delays are given:
//! - after its sentence: from the end of the whole stretch the detector cut out, which is what waiting for
//!   the model adds, as lines given out together were one sentence before it was split;
//! - after its own speech: from the end of the line itself, which is how late it feels to the people in
//!   the room, as the first part of a split sentence still waits for the rest.
//!
//! Usage: turn-delay <voice-models> <recording.wav> [setting,setting...]
//! The settings are off, slow, medium and fast; off, slow and medium unless given.

use alcyo_audio::{Ear, Latency, Recognizers, Sentence, SAMPLE_RATE};
use sherpa_onnx::Wave;
use std::cell::Cell;
use std::path::Path;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (models, wav, settings) = match args.as_slice() {
        [models, wav] => (models, wav, "off,slow,medium"),
        [models, wav, settings] => (models, wav, settings.as_str()),
        _ => anyhow::bail!("usage: turn-delay <voice-models> <recording.wav> [setting,setting...]"),
    };
    let models = Path::new(models);
    let wave = Wave::read(wav).ok_or_else(|| anyhow::anyhow!("cannot read {wav}"))?;
    let seconds = wave.samples().len() as f64 / f64::from(SAMPLE_RATE);
    println!("setting,lines,sentence_mean_s,sentence_p50_s,sentence_p90_s,line_mean_s,line_p50_s,line_p90_s,wall_s,rtf");
    for name in settings.split(',') {
        let latency = match name {
            "off" => None,
            "slow" => Some(Latency::Slow),
            "medium" => Some(Latency::Medium),
            "fast" => Some(Latency::Fast),
            "long" => Some(Latency::Long),
            other => anyhow::bail!("unknown setting {other}"),
        };
        let mut ear = Ear::with(models, Recognizers::load(models))?.turns(models, latency).wait_for_turns();
        let fed = Cell::new(0usize);
        // When each line arrived, and where it ended, in seconds.
        let mut arrivals: Vec<(f64, f64)> = vec![];
        let began = Instant::now();
        {
            let mut heard = |s: Sentence| {
                let now = fed.get() as f64 / f64::from(SAMPLE_RATE);
                arrivals.push((now, s.start + s.seconds));
            };
            for chunk in wave.samples().chunks(1600) {
                fed.set(fed.get() + chunk.len());
                ear.feed(chunk, &mut heard);
            }
        }
        let wall = began.elapsed().as_secs_f64();
        let line: Vec<f64> = arrivals.iter().map(|(now, end)| now - end).collect();
        // Lines arriving at the same moment were one sentence: each waited from the end of the last.
        let sentence: Vec<f64> = arrivals
            .iter()
            .map(|(now, _)| now - arrivals.iter().filter(|(t, _)| t == now).map(|(_, end)| *end).fold(f64::MIN, f64::max))
            .collect();
        let (s, l) = (stats(&sentence), stats(&line));
        println!("{name},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{wall:.1},{:.3}", line.len(), s.0, s.1, s.2, l.0, l.1, l.2, wall / seconds);
    }
    Ok(())
}

/// Mean, median and 90th percentile.
fn stats(values: &[f64]) -> (f64, f64, f64) {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let at = |q: f64| sorted.get(((sorted.len() as f64 - 1.0) * q).round() as usize).copied().unwrap_or(0.0);
    (values.iter().sum::<f64>() / values.len().max(1) as f64, at(0.5), at(0.9))
}
