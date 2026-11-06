//! Finding what ships with the app. An install keeps it in `<prefix>/lib/boardcast`, next to `bin/`,
//! whichever folder the package was unpacked into. In a checkout the older app's folders serve.

use boardcast_board::glyphs::HanziSource;
use serde_json::json;
use std::path::{Path, PathBuf};

pub struct Resources {
    pub hanzi: HanziSource,
    /// `llama-server` and its libraries.
    pub binaries: PathBuf,
    /// The speech models.
    pub voice: PathBuf,
    pub installed: bool,
}

fn installed_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("BOARDCAST_RESOURCES") {
        return Some(PathBuf::from(dir));
    }
    // Canonical, so a symlink on PATH still leads back to the real install.
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let dir = exe.parent()?.parent()?.join("lib/boardcast");
    dir.join("glyphs.pack").is_file().then_some(dir)
}

/// A boardcast checkout above this executable, as when running from `target/`.
fn checkout_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    exe.ancestors().find(|d| d.join("src-tauri/tauri.conf.json").is_file()).map(Path::to_path_buf)
}

pub fn locate(data: &Path) -> Resources {
    if let Some(dir) = installed_dir() {
        return Resources { hanzi: HanziSource::Pack(dir.join("glyphs.pack")), binaries: dir.join("llama"), voice: dir.join("voice"), installed: true };
    }
    let root = checkout_dir().unwrap_or_else(|| PathBuf::from("."));
    let bundled = root.join("src-tauri/voice");
    Resources {
        hanzi: HanziSource::Directory(root.join("public/hanzi")),
        binaries: root.join("src-tauri/llama"),
        voice: if bundled.join("silero_vad.onnx").is_file() { bundled } else { data.join("models/voice") },
        installed: false,
    }
}

impl Resources {
    fn hanzi_path(&self) -> &Path {
        match &self.hanzi {
            HanziSource::Pack(path) | HanziSource::Directory(path) => path,
        }
    }

    /// Whether everything an install promises is present, as JSON for packaging checks.
    pub fn self_check(&self) -> (bool, String) {
        let checks = [
            ("glyphs", self.hanzi_path().to_path_buf()),
            ("llama-server", self.binaries.join("llama-server")),
            ("voice activity model", self.voice.join("silero_vad.onnx")),
            ("speech model", self.voice.join("sense-voice/model.int8.onnx")),
            ("speech tokens", self.voice.join("sense-voice/tokens.txt")),
        ];
        let rows: Vec<_> = checks.iter().map(|(name, path)| json!({ "name": name, "path": path, "ok": path.exists() })).collect();
        let ok = checks.iter().all(|(_, path)| path.exists());
        let gpus = boardcast_agent::list_gpus(&self.binaries);
        let report = json!({
            "version": boardcast_update::VERSION,
            "installed": self.installed,
            "ok": ok,
            "checks": rows,
            "gpus": gpus.iter().map(|g| json!({ "id": g.id, "name": g.name, "mib": g.total_mib, "discrete": g.discrete })).collect::<Vec<_>>(),
        });
        (ok, report.to_string())
    }
}
