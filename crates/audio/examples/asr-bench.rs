//! How long recognition takes per sentence, with and without routing English to Parakeet.
//! Usage: asr-bench <voice-models> <cases.json> [rounds]
//! Each recording is recognized `rounds` times (default 5) by each engine, after one untimed warm-up.

use boardcast_audio::{is_english, load_parakeet, load_sense_voice, transcribe};
use serde_json::Value;
use sherpa_onnx::Wave;
use std::path::Path;
use std::time::Instant;

fn rss_mb() -> u64 {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok())
        .map_or(0, |pages| pages * 4096 / 1_000_000)
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    sorted[((sorted.len() as f64 - 1.0) * p).round() as usize]
}

fn report(name: &str, mut ms: Vec<f64>, audio_s: f64) {
    ms.sort_by(f64::total_cmp);
    let total: f64 = ms.iter().sum();
    println!(
        "{name:<22} p50 {:>6.0} ms   p95 {:>6.0} ms   max {:>6.0} ms   real-time factor {:.3}",
        percentile(&ms, 0.5),
        percentile(&ms, 0.95),
        ms[ms.len() - 1],
        total / 1000.0 / audio_s
    );
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (models, cases) = (Path::new(&args[0]), &args[1]);
    let rounds: usize = args.get(2).and_then(|r| r.parse().ok()).unwrap_or(5);
    let before = rss_mb();
    let t = Instant::now();
    let sense = load_sense_voice(&models.join("sense-voice"))?;
    let sense_load = t.elapsed();
    let with_sense = rss_mb();
    let t = Instant::now();
    let parakeet =
        load_parakeet(&models.join("parakeet-en")).ok_or_else(|| anyhow::anyhow!("no Parakeet"))?;
    let parakeet_load = t.elapsed();
    let with_both = rss_mb();
    println!(
        "loading: SenseVoice {:.1} s, +{} MB; Parakeet {:.1} s, +{} MB (resident {} MB)",
        sense_load.as_secs_f64(),
        with_sense - before,
        parakeet_load.as_secs_f64(),
        with_both - with_sense,
        with_both
    );

    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(cases)?)?;
    let waves: Vec<(String, Vec<f32>)> = cases
        .iter()
        .filter_map(|c| {
            Some((
                c["language"].as_str()?.to_string(),
                Wave::read(c["wav"].as_str()?)?.samples().to_vec(),
            ))
        })
        .collect();
    let (_, first) = &waves[0];
    transcribe(&sense, first);
    transcribe(&parakeet, first);

    let (mut s_ms, mut p_ms, mut routed_ms, mut decide_ns) = (vec![], vec![], vec![], vec![]);
    let (mut audio_s, mut english_audio_s) = (0.0, 0.0);
    for _ in 0..rounds {
        for (language, samples) in &waves {
            let seconds = samples.len() as f64 / 16000.0;
            let t = Instant::now();
            let text = transcribe(&sense, samples);
            let s = t.elapsed().as_secs_f64() * 1000.0;
            let t = Instant::now();
            let english = is_english(&text);
            decide_ns.push(t.elapsed().as_nanos() as f64);
            let mut total = s;
            if english {
                let t = Instant::now();
                transcribe(&parakeet, samples);
                let p = t.elapsed().as_secs_f64() * 1000.0;
                total += p;
                p_ms.push(p);
                english_audio_s += seconds;
            }
            s_ms.push(s);
            routed_ms.push(total);
            audio_s += seconds;
            let _ = language;
        }
    }
    println!(
        "{} recordings x {rounds} rounds, {:.0} s of audio, {:.0} s of it routed to Parakeet",
        waves.len(),
        audio_s,
        english_audio_s
    );
    report("SenseVoice, each", s_ms, audio_s);
    report("Parakeet, English only", p_ms, english_audio_s);
    report("routed, each", routed_ms, audio_s);
    decide_ns.sort_by(f64::total_cmp);
    println!(
        "routing decision       p50 {:.0} ns   max {:.0} ns",
        percentile(&decide_ns, 0.5),
        decide_ns[decide_ns.len() - 1]
    );

    // The decision alone, on many calls, to show it costs nothing next to recognition.
    let samples = [
        "小程序把请求发给后端的异步 API，拿到一个 task ID。",
        "If the token has expired, we refresh it and retry once.",
        "Account 下面有 User API 和 Consumer API。",
    ];
    let n = 1_000_000;
    let t = Instant::now();
    let mut english = 0;
    for i in 0..n {
        english += usize::from(is_english(std::hint::black_box(samples[i % 3])));
    }
    println!(
        "routing decision, {n} calls: {:.0} ns each ({english} English)",
        t.elapsed().as_nanos() as f64 / f64::from(n as u32)
    );
    println!("resident after all: {} MB", rss_mb());
    Ok(())
}
