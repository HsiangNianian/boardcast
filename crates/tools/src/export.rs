//! Write a saved meeting out, as the app's Export panel does: `export <root> <meeting.json> <out>...`
//! The format follows each file's extension.

use anyhow::{bail, Context, Result};
use boardcast_board::glyphs::{Glyphs, HanziSource};
use boardcast_export::{freeze, write_file, Kind};
use boardcast_meetings::{Live, Loaded, Origin};
use std::path::{Path, PathBuf};

pub fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    let root = PathBuf::from(args.next().context("usage: export <root> <meeting.json> <out>...")?);
    let meeting = PathBuf::from(args.next().context("usage: export <root> <meeting.json> <out>...")?);
    let raw: serde_json::Value = serde_json::from_slice(&std::fs::read(&meeting)?)?;
    let serde_json::Value::Object(raw) = raw else { bail!("{} is not a meeting", meeting.display()) };
    let mut glyphs = Glyphs::from_checkout(&root)?;
    for out in args {
        let path = Path::new(&out);
        let kind = Kind::from_path(path).with_context(|| format!("no format for {out}"))?;
        let live = Live::from_loaded(Loaded { raw: raw.clone(), origin: Origin::Current, revision: 0 });
        let output = freeze(&live, false, &mut glyphs, HanziSource::Directory(root.join("public/hanzi"))).render(kind)?;
        write_file(path, &output.bytes)?;
        println!("{{\"file\":{},\"bytes\":{},\"text_left_out\":{}}}", serde_json::Value::from(out.as_str()), output.bytes.len(), output.text_left_out);
    }
    Ok(())
}
