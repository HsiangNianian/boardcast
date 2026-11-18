//! Hear a recording as the app does, and print each sentence with the recognizer that wrote it.
//! Usage: hear-file <voice-models> <recording.wav> [off|slow|medium|fast]
//! The last says whether sentences are split where the speaker changes, and at which latency; each line
//! shows its start, length and the diarization model's speaker slot in brackets.

use boardcast_audio::{Ear, Latency, Sentence};
use sherpa_onnx::Wave;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (models, wav, turns) = match args.as_slice() {
        [models, wav] => (models, wav, "slow"),
        [models, wav, turns] => (models, wav, turns.as_str()),
        _ => anyhow::bail!("usage: hear-file <voice-models> <recording.wav> [off|slow|medium|fast]"),
    };
    let latency = match turns {
        "off" => None,
        "slow" => Some(Latency::Slow),
        "medium" => Some(Latency::Medium),
        "fast" => Some(Latency::Fast),
        other => anyhow::bail!("unknown setting {other}"),
    };
    let mut ear = Ear::load(Path::new(models))?.turns(Path::new(models), latency);
    let wave = Wave::read(wav).ok_or_else(|| anyhow::anyhow!("cannot read {wav}"))?;
    let mut heard = |s: Sentence| {
        let slot = s.slot.map_or("-".to_string(), |n| n.to_string());
        println!("{:7.2}s +{:5.2} [{slot}] {:?} {} ms: {}", s.start, s.seconds, s.engine, s.asr_ms, s.text)
    };
    for chunk in wave.samples().chunks(1600) {
        ear.feed(chunk, &mut heard);
    }
    ear.finish(&mut heard);
    Ok(())
}
