//! Finding what ships with the app. An install keeps it in `<prefix>/lib/boardcast`, next to `bin/`, or a Mac app in `Contents/Resources/boardcast`,
//! whichever folder the package was unpacked into. A checkout has them in `resources/`.

use boardcast_board::glyphs::HanziSource;
use serde_json::json;
use std::path::{Path, PathBuf};

pub struct Resources {
    pub hanzi: HanziSource,
    /// `llama-server` and its libraries.
    pub binaries: PathBuf,
    /// The speech models.
    pub voice: PathBuf,
    /// The PDF library.
    pub pdfium: PathBuf,
    pub installed: bool,
}

fn installed_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("BOARDCAST_RESOURCES") {
        return Some(PathBuf::from(dir));
    }
    // Canonical, so a symlink on PATH still leads back to the real install.
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    // <prefix>/bin/boardcast beside <prefix>/lib/boardcast, or inside an app on a Mac.
    let top = exe.parent()?.parent()?;
    [top.join("lib/boardcast"), top.join("Resources/boardcast")].into_iter().find(|dir| dir.join("glyphs.pack").is_file())
}

/// A boardcast checkout above this executable, as when running from `target/`.
fn checkout_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    exe.ancestors().find(|d| d.join("crates/ui/Cargo.toml").is_file()).map(Path::to_path_buf)
}

pub fn locate(data: &Path) -> Resources {
    if let Some(dir) = installed_dir() {
        return Resources { hanzi: HanziSource::Pack(dir.join("glyphs.pack")), binaries: dir.join("llama"), pdfium: dir.join("pdfium"), voice: dir.join("voice"), installed: true };
    }
    let root = checkout_dir().unwrap_or_else(|| PathBuf::from("."));
    let bundled = root.join("resources/voice");
    Resources {
        hanzi: HanziSource::Directory(root.join("assets/hanzi")),
        binaries: root.join("resources/llama"),
        pdfium: root.join("target/pdfium"),
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
        // Packages differ in their speech models; the variant says which ones this one promises.
        let variant = self.voice.parent().map(boardcast_update::variant_of).unwrap_or_default();
        let model = |name: &'static str, file: &str| (name, self.voice.join(file));
        let sense_voice = [model("speech model", "sense-voice/model.int8.onnx"), model("speech tokens", "sense-voice/tokens.txt")];
        let english = [model("English speech model", "parakeet-en/encoder.int8.onnx")];
        let japanese = [model("Japanese speech model", "reazon-ja/encoder-epoch-99-avg-1.int8.onnx")];
        let speech: Vec<(&str, PathBuf)> = match variant.as_str() {
            "ja" => japanese.into(),
            "en" => english.into(),
            "sensevoice" => sense_voice.into(),
            "apple" => vec![("Apple speech helper", std::env::current_exe().ok().and_then(|exe| exe.parent().map(|d| d.join("boardcast-speech"))).unwrap_or_default())],
            _ => sense_voice.into_iter().chain(english).chain(japanese).collect(),
        };
        let checks: Vec<(&str, PathBuf)> = [
            ("glyphs", self.hanzi_path().to_path_buf()),
            ("llama-server", self.binaries.join("llama-server")),
            ("voice activity model", self.voice.join("silero_vad.onnx")),
            ("speaker model", self.voice.join("speaker/campplus_zh_en.onnx")),
            ("PDF library", boardcast_import::pdf::library_file(&self.pdfium)),
        ]
        .into_iter()
        .chain(speech)
        .collect();
        let rows: Vec<_> = checks.iter().map(|(name, path)| json!({ "name": name, "path": path, "ok": path.exists() })).collect();
        let ok = checks.iter().all(|(_, path)| path.exists());
        let gpus = boardcast_agent::list_gpus(&self.binaries);
        let report = json!({
            "version": boardcast_update::VERSION,
            "installed": self.installed,
            "variant": if variant.is_empty() { "full" } else { variant.as_str() },
            "ok": ok,
            "checks": rows,
            "gpus": gpus.iter().map(|g| json!({ "id": g.id, "name": g.name, "mib": g.total_mib, "discrete": g.discrete })).collect::<Vec<_>>(),
        });
        (ok, report.to_string())
    }
}
