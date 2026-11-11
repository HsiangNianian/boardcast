//! Hear a recording as the app does, and print each sentence with the recognizer that wrote it.
//! Usage: hear-file <voice-models> <recording.wav>

use boardcast_audio::{Ear, Sentence};
use sherpa_onnx::Wave;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [models, wav] = args.as_slice() else {
        anyhow::bail!("usage: hear-file <voice-models> <recording.wav>");
    };
    let mut ear = Ear::load(Path::new(models))?;
    let wave = Wave::read(wav).ok_or_else(|| anyhow::anyhow!("cannot read {wav}"))?;
    let mut heard = |s: Sentence| {
        let timing = s.asr_ms.map_or("streaming".into(), |ms| format!("{ms} ms"));
        println!("{:7.2}s {:?} {timing}: {}", s.start, s.engine, s.text);
    };
    for chunk in wave.samples().chunks(1600) {
        ear.feed(chunk, &mut heard);
    }
    ear.finish(&mut heard);
    Ok(())
}
