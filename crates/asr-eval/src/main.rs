//! Measures the app's speech recognition on a fixed corpus of Chinese, Japanese and English recordings.
//!
//!   asr-eval download <root> [--suite quick|full|meetings]
//!       Fetches the suite's recordings into eval/asr/.data and checks each one, cached or not, against its
//!       size and SHA-256. Writes index.html there, a page to hear them beside their references.
//!   asr-eval run <root> [--suite quick|full|meetings] [--engine routed|sensevoice] [--vad on|off]
//!                [--enhance none|gain] [--only id,id] [--output dir]
//!       Hears each recording as the app does and writes report.json and report.md, to eval/reports/asr/ and
//!       the GitHub Actions job summary. The configuration the app ships fails the run when speech goes
//!       unheard or silence is heard as words; the controls only report it.
//!   asr-eval compare <report.json>...
//!       The runs' language totals side by side.
//!   asr-eval speakers <root> [--only id,id] [--output dir] [--reuse]
//!       How well speakers are told apart on the meeting recordings, at several thresholds; see speakers.rs.
//!
//! The speech models are read from <root>/resources/voice, where the packages take them from too.

mod corpus;
mod run;
mod score;
mod speakers;

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_default();
    if command == "compare" {
        return run::compare(&args.map(PathBuf::from).collect::<Vec<_>>());
    }
    let root = PathBuf::from(args.next().context("the boardcast checkout")?);
    let mut options = run::Options {
        suite: "quick".into(),
        engine: "routed".into(),
        vad: true,
        enhance: "none".into(),
        only: None,
        output: None,
    };
    let mut reuse = false;
    while let Some(flag) = args.next() {
        let mut value = || args.next().with_context(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--suite" => options.suite = value()?,
            "--engine" => options.engine = value()?,
            "--vad" => {
                options.vad = match value()?.as_str() {
                    "on" => true,
                    "off" => false,
                    other => bail!("--vad is on or off, not {other}"),
                }
            }
            "--enhance" => options.enhance = value()?,
            "--only" => options.only = Some(value()?.split(',').map(str::to_string).collect()),
            "--output" => options.output = Some(PathBuf::from(value()?)),
            "--reuse" => reuse = true,
            other => bail!("unknown flag {other}"),
        }
    }
    match command.as_str() {
        "download" => corpus::prepare(&root, &corpus::load(&root, &options.suite)?),
        "run" => run::run(&root, &options),
        "speakers" => speakers::run(
            &root,
            options.only.as_deref(),
            &options
                .output
                .clone()
                .unwrap_or_else(|| root.join("eval/reports/speakers")),
            reuse,
        ),
        _ => {
            bail!("usage: asr-eval download|run <root> [flags] | asr-eval compare <report.json>...")
        }
    }
}
