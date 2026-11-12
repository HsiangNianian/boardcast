//! A sheet of the board's handwriting, to see how its hand writes.
//!
//!   ink-sample <root> <out.png> [--size <px>] [--messy <0..1>]... <text>...
//!
//! Each text is written on a row of its own, as the board writes a label of that size. With several
//! `--messy`, all the texts are written at each messiness in turn, the neatest first.

use anyhow::{bail, Context, Result};
use boardcast_board::board_layout::Bounds;
use boardcast_board::compose::handwriting;
use boardcast_board::glyphs::Glyphs;
use boardcast_board::pen::{outline, Rng};
use boardcast_export::ink::{png, Drawn, Shape};
use std::path::PathBuf;

pub fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    let root = PathBuf::from(args.next().context("boardcast root")?);
    let out = PathBuf::from(args.next().context("output .png")?);
    let (mut size, mut messy, mut texts) = (40.0_f32, vec![], vec![]);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--size" => size = args.next().context("a size")?.parse()?,
            "--messy" => messy.push(args.next().context("a messiness")?.parse::<f32>()?),
            flag if flag.starts_with("--") => bail!("unknown flag {flag}"),
            _ => texts.push(arg),
        }
    }
    if texts.is_empty() {
        bail!("no text to write");
    }
    if messy.is_empty() {
        messy.push(0.0);
    }
    let mut glyphs = Glyphs::from_checkout(&root)?;
    let (mut strokes, mut y, mut right) = (vec![], 0.0_f32, 0.0_f32);
    for m in messy {
        glyphs.set_messy(m);
        for text in &texts {
            // Seeded as the board seeds a piece, and inked in the same order, so the sheet matches the board.
            let seed = format!("{text}|{m}");
            let mut r = Rng::new(&seed);
            for line in handwriting(&mut glyphs, text, 0.0, y, size, &seed) {
                let shape = outline(&line.inked(&mut r), 1.0);
                right = shape.iter().map(|p| p[0]).fold(right, f32::max);
                strokes.push(Shape { outline: shape, style: line.style });
            }
            y += size * 1.6;
        }
    }
    let bounds = Bounds { x0: 0.0, y0: 0.0, x1: f64::from(right), y1: f64::from(y - size * 0.4) };
    std::fs::write(&out, png(&Drawn { strokes, bounds, page: None })?)?;
    println!("written to {}", out.display());
    Ok(())
}
