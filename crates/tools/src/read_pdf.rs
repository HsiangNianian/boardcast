//! How a PDF reads, page by page, to inspect on a real file: `read-pdf <pdfium-dir> <file.pdf> <out.json>`.

use anyhow::{Context, Result};
use boardcast_import::paper::{analyze_page, PaperState};
use boardcast_import::pdf::read_text;
use serde_json::{json, Value};
use std::path::PathBuf;

pub fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    let usage = "usage: read-pdf <pdfium-dir> <file.pdf> <out.json>";
    let library = PathBuf::from(args.next().context(usage)?);
    let file = PathBuf::from(args.next().context(usage)?);
    let out = PathBuf::from(args.next().context(usage)?);
    let pages = read_text(&std::fs::read(&file)?, &library)?;
    let mut state = PaperState::default();
    let rows: Vec<Value> = pages
        .iter()
        .map(|page| {
            let found = analyze_page(&page.pieces, page.w, page.h, &mut state);
            json!({
                "w": page.w,
                "h": page.h,
                "pieces": page.pieces.iter().map(|p| {
                    let mut piece = json!({ "text": p.text, "x": p.x, "y": p.y, "w": p.w, "h": p.h });
                    if let Some(angle) = p.angle {
                        piece["angle"] = json!(angle);
                    }
                    piece
                }).collect::<Vec<_>>(),
                "areas": found.areas.iter().map(|a| json!({ "kind": a.kind.name(), "text": a.area.text, "name": a.area.name, "x": a.area.x, "y": a.area.y, "w": a.area.w, "h": a.area.h, "rows": a.area.rows.as_ref().map_or(0, Vec::len) })).collect::<Vec<_>>(),
                "headings": found.headings,
                "columns": found.columns,
            })
        })
        .collect();
    std::fs::write(&out, serde_json::to_string(&rows)?)?;
    println!("{{\"pages\":{}}}", rows.len());
    Ok(())
}
