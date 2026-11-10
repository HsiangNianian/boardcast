//! Checks recorded boards from the command line; a remaining hard defect makes the exit non-zero.

use anyhow::{bail, Context, Result};
use boardcast_board::board_layout::StableLayout;
use boardcast_board::check::{check, check_and_repair, Report};
use boardcast_board::compose::compose;
use boardcast_board::glyphs::Glyphs;
use boardcast_board::graph::{parse_ops, Graph};
use boardcast_board::sugiyama::first_diagram;
use std::path::{Path, PathBuf};

fn demo_lines(root: &Path, name: &str) -> Result<Vec<String>> {
    let demo: serde_json::Value = serde_json::from_slice(&std::fs::read(
        root.join(format!("assets/demos/{name}.json")),
    )?)?;
    Ok(demo["lines"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| l["ops"].as_str().map(str::to_string))
        .collect())
}

pub fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    let root = PathBuf::from(args.next().context("boardcast root")?);
    let mut sets: Vec<(String, Vec<String>)> = vec![];
    let mut tidy = false;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--all-demos" => {
                let mut names: Vec<String> = std::fs::read_dir(root.join("assets/demos"))?
                    .filter_map(|e| e.ok()?.file_name().into_string().ok())
                    .filter_map(|f| f.strip_suffix(".json").map(str::to_string))
                    .collect();
                names.sort();
                for n in names {
                    let lines = demo_lines(&root, &n)?;
                    sets.push((n, lines));
                }
            }
            "--demo" => {
                let n = args.next().context("demo name")?;
                let lines = demo_lines(&root, &n)?;
                sets.push((n, lines));
            }
            "--ops" => {
                let p = args.next().context("ops file")?;
                sets.push((p.clone(), vec![std::fs::read_to_string(&p)?]));
            }
            "--tidy" => tidy = true,
            other => bail!("unknown flag {other}"),
        }
    }
    let mut glyphs = Glyphs::from_checkout(&root)?;
    let (mut hard_steps, mut steps, mut left) = (0, 0, 0);
    for (name, lines) in &sets {
        let mut g = Graph::default();
        let mut board = StableLayout::default();
        let mut last = Report::default();
        for ops in lines {
            g.apply(&parse_ops(ops));
            steps += 1;
            let (found, repaired) = if tidy {
                check_and_repair(&g, &mut board, &mut glyphs)
            } else {
                let l = board.layout(&g, &mut first_diagram);
                let (_, unclear) = compose(&g, &l, &mut glyphs);
                (check(&g, &l, &unclear), None)
            };
            hard_steps += usize::from(!found.hard.is_empty());
            last = repaired.unwrap_or(found);
            left += last.hard.len();
        }
        println!(
            "{{\"board\":{},\"report\":{}}}",
            serde_json::Value::from(name.clone()),
            last.to_json()
        );
    }
    println!("{{\"boards\":{},\"steps\":{steps},\"steps_with_hard_defect\":{hard_steps},\"hard_after_repair\":{left}}}", sets.len());
    if left > 0 {
        bail!("{left} hard layout defects remain");
    }
    Ok(())
}
