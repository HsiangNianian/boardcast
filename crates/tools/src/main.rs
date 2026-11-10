//! Board tooling with no window, for CI and model evaluation.
//!   check <root> (--all-demos | --demo <name> | --ops <file>) [--tidy]
//!   agent-eval <root> <output.json> [--deepseek] [--llm <url>] [--skill | --rules-only | --hints-only]
//!              [--only-dev | --only-holdout] [--only <id>] [--workers <n>]
//!   pack-glyphs <assets/hanzi> <glyphs.pack>
//!   download-model <id> <dir>
//!   version-newer <new> <old>   Fails unless <new> is a later version than <old>.
//!   meetings-check <data-dir>   Rewrites every meeting into <data-dir>'s v2 folder; use on a copy.

mod agent_eval;
mod check;
mod export;
mod read_pdf;
mod meetings;

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("check") => check::run(args),
        Some("agent-eval") => agent_eval::run(args),
        Some("meetings-check") => meetings::run(args),
        Some("export") => export::run(args),
        Some("read-pdf") => read_pdf::run(args),
        Some("pack-glyphs") => pack_glyphs(args),
        Some("download-model") => download_model(args),
        Some("version-newer") => version_newer(args),
        _ => bail!("usage: check <root> ... | agent-eval <root> <output.json> ..."),
    }
}

fn pack_glyphs(mut args: impl Iterator<Item = String>) -> Result<()> {
    let dir = PathBuf::from(args.next().context("source folder")?);
    let out = PathBuf::from(args.next().context("output file")?);
    let count = boardcast_board::glyphs::write_pack(&dir, &out)?;
    println!("{count} characters in {}", out.display());
    Ok(())
}

fn download_model(mut args: impl Iterator<Item = String>) -> Result<()> {
    let id = args.next().context("model id")?;
    let dir = PathBuf::from(args.next().context("folder")?);
    let model = boardcast_models::catalog().into_iter().find(|m| m.id == id).with_context(|| format!("no model {id}"))?;
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let mut last = 0;
    let outcome = boardcast_models::download(&model, &dir, &cancel, &mut |p| {
        let percent = p.done * 100 / p.total.max(1);
        if percent != last {
            last = percent;
            println!("{percent}% {} MB/s {}", p.speed / 1_000_000, p.source.unwrap_or_default());
        }
    })?;
    println!("{outcome:?}");
    Ok(())
}

fn version_newer(mut args: impl Iterator<Item = String>) -> Result<()> {
    let new = semver::Version::parse(&args.next().context("new version")?)?;
    let old = semver::Version::parse(&args.next().context("old version")?)?;
    if new <= old {
        bail!("{new} is not newer than {old}");
    }
    Ok(())
}
