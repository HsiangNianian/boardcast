//! boardcast on GPUI: talk, and the whiteboard draws itself.
//!   boardcast [--open <id>] [--deepseek] [--llm <url>] [--skill]
//!   boardcast --version | --self-check | --update
//! For development, with a checkout as <root>:
//!   boardcast --root <root> [--case <id>] [--wav <path>] [--exit-when-done] [--demo <name>]
//!             [--synthetic <chars>] [--zoom-test] [--zoom <k>]
//!             [--panel library|summary|export|import|speakers|settings]
//!   boardcast bench <root> <stroke|merged:N|raster> <speed> <targets,..> [marks]

// No console window behind the app on Windows; the command-line flags attach to the one they were run from.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod bench;
mod i18n;
mod listen;
mod pipeline;
mod raster;
mod resources;
mod splash;
mod theme;

use anyhow::{bail, Result};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1).peekable();
    #[cfg(windows)]
    if args.peek().is_some_and(|a| a.starts_with("--")) {
        // SAFETY: a plain call; it fails harmlessly when there is no console to attach to.
        unsafe { windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS) };
    }
    match args.peek().map(String::as_str) {
        Some("--version") => {
            println!("boardcast {}", boardcast_update::VERSION);
            Ok(())
        }
        Some("--update") => update(),
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

/// Update from the command line, the same way the window does.
fn update() -> Result<()> {
    let Some(prefix) = boardcast_update::installed_prefix() else { bail!("this copy was not installed from a package") };
    let Some(release) = boardcast_update::check()? else {
        println!("boardcast {} is up to date", boardcast_update::VERSION);
        return Ok(());
    };
    println!("downloading {}", release.version);
    let dir = boardcast_meetings::default_dir()?.join("updates");
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let Some(archive) = boardcast_update::fetch(&release, &dir, &cancel, &mut |_| {})? else { bail!("cancelled") };
    let result = boardcast_update::install(&archive, &release, &prefix);
    // A package that failed verification or would not run is no use to retry from.
    let _ = std::fs::remove_file(&archive);
    result?;
    println!("updated to {}", release.version);
    Ok(())
}
