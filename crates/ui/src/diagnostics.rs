//! Previewable diagnostic files; meeting text and free-form settings never enter the bundle.
use anyhow::Result;
use serde_json::{json, Value};
use std::io::{Cursor, Write};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

#[derive(Clone)]
pub struct File {
    pub name: &'static str,
    pub text: String,
}

pub struct Info {
    pub screen: [f32; 3],
    pub backend: &'static str,
    pub model: Option<&'static str>,
    pub state: &'static str,
    pub installed: Vec<&'static str>,
    pub listening: bool,
    pub source: &'static str,
    pub diarize: bool,
    pub apple_helper: bool,
}

fn number(value: &Value) -> Option<u64> {
    value.as_f64().filter(|n| n.is_finite() && *n >= 0.0 && *n < u64::MAX as f64).map(|n| n.round() as u64)
}

// Keep the Tauri report's percentile ranks for these stored per-line measurements.
fn stats(mut values: Vec<u64>) -> Value {
    if values.is_empty() { return Value::Null; }
    values.sort_unstable();
    let at = |p: usize| values[(values.len() * p / 100).min(values.len() - 1)];
    json!({ "n": values.len(), "p50": at(50), "p95": at(95), "max": values.last() })
}

fn timing(lines: &[Value]) -> Value {
    let done: Vec<_> = lines.iter().filter(|l| !l["manual"].as_bool().unwrap_or(false)
        && !l["failed"].as_bool().unwrap_or(false) && number(&l["ms"]).is_some()).collect();
    let spoken: Vec<_> = done.iter().filter(|l| l["heard"].is_object()).collect();
    let stages: Vec<_> = done.iter().map(|l| [number(&l["heard"]["waitMs"]), number(&l["heard"]["asrMs"]),
        number(&l["heard"]["voiceprintMs"]), number(&l["queueMs"]), number(&l["ms"])]).collect();
    let mut report = json!({ "model": null, "lines": done.len(), "spoken": spoken.len(),
        "softBreaks": spoken.iter().map(|l| l["heard"]["soft"].as_bool().map(usize::from)).sum::<Option<usize>>(),
        "totalMs": stats(stages.iter().filter_map(|s| s.iter().try_fold(0_u64, |total, n| total.checked_add((*n)?))).collect()),
        "timing_basis": "Stored completed, non-manual, non-failed meeting lines; spoken counts lines with ASR metadata. Missing stages remain null; totalMs is a stage sum only when all five stages were measured, not acoustic-to-screen latency. Runtime capture, request and drawing populations are separate."
    });
    for (i, name) in ["waitMs", "asrMs", "voiceprintMs", "queueMs", "modelMs"].iter().enumerate() {
        report[*name] = stats(stages.iter().filter_map(|s| s[i]).collect());
    }
    report
}

pub fn files(info: Info, lines: &[Value], runtime: Value) -> Vec<File> {
    let info = json!({ "version": boardcast_update::VERSION,
        "platform": { "os": std::env::consts::OS, "arch": std::env::consts::ARCH },
        "userAgent": "GPUI", "screen": format!("{}x{}@{}", info.screen[0], info.screen[1], info.screen[2]),
        "model": { "state": info.state, "backend": info.backend, "model": info.model,
            "url": null, "device": null, "message": null, "models_dir": null, "gpus": null },
        "voice": { "listening": info.listening, "source": info.source, "diarize": info.diarize,
            "device": null, "choice": null, "error": null },
        "speechModels": { "apple_helper_available": info.apple_helper }, "installedModels": info.installed,
        "omissions": "Device names, paths, endpoints, custom model names, keys and error bodies are excluded. This GPUI build does not capture raw llama-server logs. Helper availability does not establish native speech capability or permission."
    });
    let mut errors = vec![];
    for capture in runtime["captures"].as_array().into_iter().flatten() {
        if capture["phase"] != "failed" { continue; }
        let source = match capture["failure_source"].as_str() { Some("microphone") => "microphone", Some("system") => "system", _ => "unknown" };
        errors.push(format!("capture {} failed; first reported source: {source}", capture["id"].as_u64().unwrap_or(0)));
        for (i, track) in capture["tracks"].as_array().into_iter().flatten().take(2).enumerate() {
            let stage = match track["failure_stage"].as_str() { Some("preparing") => "preparing", Some("capture") => "capture", Some("recognition") => "recognition", _ => continue };
            errors.push(format!("  {}: {stage}", crate::listen::SOURCES[i]));
        }
    }
    for request in runtime["board"]["requests"]["recent"].as_array().into_iter().flatten().filter(|r| r["outcome"] == "failed") {
        errors.push(format!("board request {} failed", request["id"].as_u64().unwrap_or(0)));
    }
    if errors.is_empty() { errors.push("No failures in the retained capture and board-request records.".into()); }
    errors.push("Only retained structured failures are listed; no error bodies or raw logs are included.".into());
    let mut timing = timing(lines);
    timing["runtime"] = runtime;
    vec![File { name: "info.json", text: serde_json::to_string_pretty(&info).expect("info serializes") },
        File { name: "timing.json", text: serde_json::to_string_pretty(&timing).expect("timing serializes") },
        File { name: "errors.txt", text: errors.join("\n") + "\n" }]
}

pub fn zip(files: &[File]) -> Result<Vec<u8>> {
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    for file in files {
        archive.start_file(file.name, SimpleFileOptions::default().compression_method(CompressionMethod::Stored))?;
        archive.write_all(file.text.as_bytes())?;
    }
    Ok(archive.finish()?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn stored_timings_keep_the_old_ranks_without_turning_missing_stages_into_zero() {
        let lines = json!([
            {"text":"private", "speaker":"private", "ms":800,"queueMs":0,"heard":{"waitMs":450,"asrMs":60,"voiceprintMs":20,"soft":false}},
            {"ms":900,"queueMs":700,"heard":{"waitMs":130,"asrMs":80,"voiceprintMs":20,"soft":true}},
            {"ms":1000,"queueMs":0}, {"manual":true,"ms":0}, {"ms":8,"failed":true}, {"text":"still waiting"}
        ]);
        let report = timing(lines.as_array().unwrap());
        assert_eq!(report["lines"], 3);
        assert_eq!(report["spoken"], 2);
        assert_eq!(report["softBreaks"], 1);
        assert_eq!(report["waitMs"], json!({"n":2,"p50":450,"p95":450,"max":450}));
        assert_eq!(report["totalMs"], json!({"n":2,"p50":1830,"p95":1830,"max":1830}));
        assert!(!report.to_string().contains("private"));
        let unknown = timing(&[json!({"ms":15,"heard":{"asrMs":5}})]);
        assert_eq!(unknown["asrMs"]["p50"], 5);
        for name in ["waitMs", "voiceprintMs", "queueMs", "totalMs", "softBreaks"] { assert!(unknown[name].is_null(), "{name}"); }
        assert!(number(&json!(-1)).is_none());
        assert!(number(&json!("secret")).is_none());
    }

    #[test]
    fn archive_contains_exactly_the_previewed_files_and_structured_failures() {
        let runtime = json!({"captures":[{"id":4,"phase":"failed","failure_source":"system","tracks":[{}, {"failure_stage":"capture"}]}],
            "board":{"requests":{"recent":[{"id":9,"outcome":"failed"}]}},"screen_present_ms":null});
        let files = files(Info { screen:[800.,600.,1.], backend:"remote",model:None,state:"configured",installed:vec![],
            listening:false,source:"both",diarize:true,apple_helper:false }, &[json!({"text":"private text", "rawText":"sk-private-key", "ms":7})], runtime.clone());
        let timing: Value = serde_json::from_str(&files[1].text).unwrap();
        assert_eq!(timing["runtime"], runtime);
        assert!(files[2].text.contains("capture 4 failed; first reported source: system"));
        assert!(files[2].text.contains("system: capture") && files[2].text.contains("board request 9 failed"));
        let bytes = zip(&files).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert_eq!(archive.len(), 3);
        for file in files {
            let mut read = String::new();
            archive.by_name(file.name).unwrap().read_to_string(&mut read).unwrap();
            assert_eq!(read, file.text);
            assert!(!read.contains("private"));
        }
    }
}
