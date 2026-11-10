//! Write a saved meeting out, as the app's Export panel does: `export <root> <meeting.json> <out>...`
//! The format follows each file's extension.

use anyhow::{bail, Context, Result};
use boardcast_board::glyphs::{Glyphs, HanziSource};
use boardcast_export::{freeze, write_file, Kind};
use boardcast_meetings::{decode_data_url, Live, Loaded, Origin, PageImage, Store};
use std::path::{Path, PathBuf};

pub fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    let root = PathBuf::from(args.next().context("usage: export <root> <meeting.json> <out>...")?);
    let meeting = PathBuf::from(args.next().context("usage: export <root> <meeting.json> <out>...")?);
    let raw: serde_json::Value = serde_json::from_slice(&std::fs::read(&meeting)?)?;
    let serde_json::Value::Object(raw) = raw else { bail!("{} is not a meeting", meeting.display()) };
    let mut glyphs = Glyphs::from_checkout(&root)?;
    // Images are kept in a folder named after the meeting, beside its file; a copy elsewhere has none.
    let store = meeting.ancestors().nth(3).map(|data| Store::new(data.to_path_buf()));
    let id = raw.get("id").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
    let images: Vec<Option<PageImage>> = Live::from_loaded(Loaded { raw: raw.clone(), origin: Origin::Current, revision: 0 })
        .board_images()
        .iter()
        .map(|r| r.as_ref().and_then(|r| store.as_ref().and_then(|s| s.page_image(&id, r).ok()).or_else(|| decode_data_url(r).ok())))
        .collect();
    for out in args {
        let path = Path::new(&out);
        let kind = Kind::from_path(path).with_context(|| format!("no format for {out}"))?;
        let live = Live::from_loaded(Loaded { raw: raw.clone(), origin: Origin::Current, revision: 0 });
        let output = freeze(&live, false, &mut glyphs, HanziSource::Directory(root.join("assets/hanzi")), &images).render(kind)?;
        write_file(path, &output.bytes)?;
        println!("{{\"file\":{},\"bytes\":{},\"text_left_out\":{}}}", serde_json::Value::from(out.as_str()), output.bytes.len(), output.text_left_out);
    }
    Ok(())
}
