//! Board tooling with no window, for CI and model evaluation.
//!   check <root> (--all-demos | --demo <name> | --ops <file>) [--tidy]
//!   agent-eval <root> <output.json> [--set dev|holdout|pointing|all] [--repeat N] [--label name] [--deepseek | --llm <url>] [--skill | --rules-only | --hints-only]
//!              [--only-dev | --only-holdout] [--only <id>] [--workers <n>]
//!   fetch voice | pdfium [platform] | llama [platform] | glyphs
//!   pack-glyphs <assets/hanzi> <glyphs.pack> [--only-from <file>...]   Only the characters those files use.
//!   download-model <id> <dir>
//!   sign <file>...              Writes <file>.sig with the update key; see release.rs.
//!   release-manifest <packages> <tag>
//!   version-newer <new> <old>   Fails unless <new> is a later version than <old>.
//!   meetings-check <data-dir>   Rewrites every meeting into <data-dir>'s v2 folder; use on a copy.
//!   search-eval <root> <model.gguf> <gemma|qwen3> [--out <report.json>]   Meeting search on eval/search/queries.json.

mod agent_eval;
mod check;
mod export;
mod fetch;
mod read_pdf;
mod release;
mod score;
mod search_eval;
mod summary_eval;
mod meetings;
mod voice_files;

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("check") => check::run(args),
        Some("agent-eval") => agent_eval::run(args),
        Some("summary-eval") => summary_eval::run(args),
        Some("meetings-check") => meetings::run(args),
        Some("export") => export::run(args),
        Some("read-pdf") => read_pdf::run(args),
        Some("fetch") => fetch::run(args),
        Some("sign") => release::sign(args),
        Some("release-manifest") => release::manifest(args),
        Some("pack-glyphs") => pack_glyphs(args),
        Some("download-model") => download_model(args),
        Some("version-newer") => version_newer(args),
        Some("search-eval") => search_eval::run(args),
        _ => bail!("usage: check <root> ... | agent-eval <root> <output.json> ..."),
    }
}

fn pack_glyphs(mut args: impl Iterator<Item = String>) -> Result<()> {
    let dir = PathBuf::from(args.next().context("source folder")?);
    let out = PathBuf::from(args.next().context("output file")?);
    let rest: Vec<String> = args.collect();
    let only = match rest.split_first() {
        Some((flag, files)) if flag == "--only-from" => {
            let mut keep = std::collections::HashSet::new();
            for f in files {
                keep.extend(std::fs::read_to_string(f).with_context(|| format!("reading {f}"))?.chars().filter(|c| *c as u32 >= 0x2E80));
            }
            Some(keep)
        }
        Some((other, _)) => anyhow::bail!("unknown flag {other}"),
        None => None,
    };
    let count = boardcast_board::glyphs::write_pack_of(&dir, &out, only.as_ref())?;
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
