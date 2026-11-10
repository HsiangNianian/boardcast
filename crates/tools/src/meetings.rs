//! Opens and saves every meeting in a data directory, and reports anything that changed beyond the
//! fields a save owns. It writes v2 copies, so point it at a copy of real data.

use anyhow::{bail, Context, Result};
use boardcast_meetings::{Live, Store};
use serde_json::{json, Value};
use std::path::PathBuf;

/// Saving legitimately sets these; everything else must come through unchanged.
const OWNED: [&str; 7] = [
    "version",
    "storageRevision",
    "migration",
    "updatedAt",
    "steps",
    "lines",
    "layout",
];

pub fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    let root = PathBuf::from(args.next().context("data directory")?);
    let store = Store::new(root);
    let (mut saved, mut viewed, mut bad) = (0, 0, 0);
    for info in store.list()? {
        if let Some(error) = &info.read_error {
            println!("{}", json!({ "id": info.id, "unreadable": error }));
            bad += 1;
            continue;
        }
        let loaded = store.load(&info.id)?.context("listed but not loadable")?;
        let before = Value::Object(loaded.raw.clone());
        let hash = match &loaded.origin {
            boardcast_meetings::Origin::Legacy { source_hash } => Some(source_hash.clone()),
            _ => None,
        };
        let mut live = Live::from_loaded(loaded);
        if let Some(reason) = live.read_only {
            println!(
                "{}",
                json!({ "id": info.id, "viewOnly": format!("{reason:?}"), "sourceHash": hash })
            );
            viewed += 1;
            continue;
        }
        let snapshot = live.snapshot();
        let mut changed: Vec<String> = snapshot
            .keys()
            .filter(|k| {
                !OWNED.contains(&k.as_str()) && before.get(k.as_str()) != snapshot.get(k.as_str())
            })
            .cloned()
            .collect();
        changed.extend(
            before
                .as_object()
                .into_iter()
                .flat_map(|m| m.keys())
                .filter(|k| !snapshot.contains_key(k.as_str()))
                .cloned(),
        );
        let origin = live.origin.clone();
        let revision = store.save(&info.id, snapshot, &origin, live.revision)?;
        live.saved(revision);
        let again = Live::from_loaded(store.load(&info.id)?.context("just saved")?);
        let same =
            again.graph == live.graph && again.lines == live.lines && again.steps == live.steps;
        println!(
            "{}",
            json!({ "id": info.id, "sourceHash": hash, "changed": changed, "reopenedSame": same, "nodes": again.graph.nodes.len() })
        );
        if !changed.is_empty() || !same {
            bad += 1;
        }
        saved += 1;
    }
    println!(
        "{}",
        json!({ "saved": saved, "viewOnly": viewed, "problems": bad })
    );
    if bad > 0 {
        bail!("{bad} meetings had problems");
    }
    Ok(())
}
