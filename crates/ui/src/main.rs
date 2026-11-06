//! boardcast on GPUI: talk, and the whiteboard draws itself.
//!   app <root> [--open <id>] [--case <id>] [--wav <path>] [--deepseek] [--llm <url>] [--skill]
//!              [--exit-when-done] [--demo <name>] [--synthetic <chars>] [--zoom-test] [--zoom <k>]
//!   bench <root> <stroke|merged:N|raster> <speed> <targets,..> [marks]

mod app;
mod bench;
mod i18n;
mod pipeline;
mod raster;

use anyhow::{bail, Result};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("bench") => bench::run(args),
        Some("app") => app::run(args),
        _ => bail!("usage: app <root> ... | bench <root> ..."),
    }
}
