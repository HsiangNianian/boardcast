//! boardcast on GPUI: talk, and the whiteboard draws itself.
//!   boardcast [--open <id>] [--deepseek] [--llm <url>] [--skill]
//!   boardcast --version | --self-check
//! For development, with a checkout as <root>:
//!   boardcast --root <root> [--case <id>] [--wav <path>] [--exit-when-done] [--demo <name>]
//!             [--synthetic <chars>] [--zoom-test] [--zoom <k>]
//!   boardcast bench <root> <stroke|merged:N|raster> <speed> <targets,..> [marks]

mod app;
mod bench;
mod i18n;
mod pipeline;
mod raster;
mod resources;

use anyhow::{bail, Result};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1).peekable();
    match args.peek().map(String::as_str) {
        Some("--version") => {
            println!("boardcast {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("--self-check") => {
            let (ok, report) = resources::locate(&boardcast_meetings::default_dir()?).self_check();
            println!("{report}");
            if ok { Ok(()) } else { bail!("this install is incomplete") }
        }
        Some("bench") => {
            args.next();
            bench::run(args)
        }
        _ => app::run(args),
    }
}
