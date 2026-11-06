//! Board tooling with no window, for CI and model evaluation.
//!   check <root> (--all-demos | --demo <name> | --ops <file>) [--tidy]
//!   agent-eval <root> <output.json> [--deepseek] [--llm <url>] [--skill | --rules-only | --hints-only]
//!              [--only-dev | --only-holdout] [--only <id>] [--workers <n>]

mod agent_eval;
mod check;

use anyhow::{bail, Result};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("check") => check::run(args),
        Some("agent-eval") => agent_eval::run(args),
        _ => bail!("usage: check <root> ... | agent-eval <root> <output.json> ..."),
    }
}
