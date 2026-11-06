//! Meeting files: `meetings/` holds the older v1 format and `meetings/v2/` the current one.
//! A v1 original is never written to; its first save goes to v2 with a hash of the original, as the
//! older app does. Writes are atomic and locked, and refuse to overwrite a change made elsewhere.

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MEETING_VERSION: u64 = 2;

/// Where the older app keeps its data, unless `BOARDCAST_DATA` says otherwise.
pub fn default_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("BOARDCAST_DATA") {
        return Ok(PathBuf::from(dir));
    }
    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        return Ok(PathBuf::from(dir).join("games.mmstudio.boardcast"));
    }
    Ok(PathBuf::from(std::env::var("HOME").context("HOME is not set")?).join(".local/share/games.mmstudio.boardcast"))
}

/// Time plus a little randomness, using only characters that are safe in a file name.
pub fn new_id() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = now.as_secs() as i64;
    let (days, rest) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    // Civil date from a day count (Howard Hinnant's algorithm), in UTC.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let mut seed = now.subsec_nanos();
    let tail: String = (0..4)
        .map(|_| {
            seed = seed.wrapping_mul(2_654_435_761).wrapping_add(12_345);
            char::from_digit((seed >> 16) % 36, 36).unwrap_or('0')
        })
        .collect();
    format!("{year:04}{month:02}{day:02}-{:02}{:02}{:02}-{tail}", rest / 3600, rest % 3600 / 60, rest % 60)
}

fn valid(id: &str) -> Result<()> {
    if !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        Ok(())
    } else {
        Err(anyhow!("invalid meeting id: {id}"))
    }
}

/// What a library row shows.
#[derive(Clone, Debug)]
pub struct Info {
    pub id: String,
    pub title: String,
    pub created_at: f64,
    pub updated_at: f64,
    pub lines: usize,
    pub pages: usize,
    /// Unreadable files stay listed, so a damaged meeting is visible rather than silently gone.
    pub read_error: Option<String>,
}

fn info(v: &Value) -> Option<Info> {
    Some(Info {
        id: v.get("id")?.as_str()?.to_string(),
        title: v.get("title").and_then(Value::as_str).unwrap_or("Untitled meeting").to_string(),
        created_at: v.get("createdAt").and_then(Value::as_f64).unwrap_or(0.0),
        updated_at: v.get("updatedAt").and_then(Value::as_f64).unwrap_or(0.0),
        lines: v["lines"].as_array().map_or(0, |ls| ls.iter().filter(|l| !l["manual"].as_bool().unwrap_or(false)).count()),
        pages: v["pages"].as_array().map_or(1, |p| p.len().max(1)),
        read_error: None,
    })
}

/// Where a loaded meeting came from, which decides how its next save is written.
#[derive(Clone, Debug, PartialEq)]
pub enum Origin {
    /// Not saved yet.
    New,
    /// A v1 file, with the hash of its text as `JSON.stringify` would produce it.
    Legacy { source_hash: String },
    Current,
}

pub struct Loaded {
    pub raw: Map<String, Value>,
    pub origin: Origin,
    pub revision: u64,
}

#[derive(Clone, Debug)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn legacy_dir(&self) -> PathBuf {
        self.root.join("meetings")
    }

    fn current_dir(&self) -> PathBuf {
        self.root.join("meetings/v2")
    }

    /// The older app hashes `JSON.stringify(parsed)`, which for its own files is exactly their text.
    /// Hashing the text avoids any disagreement over how a number is written.
    pub fn source_hash(text: &str) -> String {
        Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
    }

    fn deleted(dir: &Path, id: &str) -> bool {
        dir.join(format!("{id}.deleted")).exists()
    }

    fn scan(dir: &Path, out: &mut Vec<Info>) -> Result<()> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
        };
        for entry in entries {
            let path = entry?.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let id = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            if Self::deleted(dir, &id) || out.iter().any(|m| m.id == id) {
                continue;
            }
            let read = fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|e| e.to_string()))
                .and_then(|v| info(&v).filter(|m| m.id == id).ok_or_else(|| "no matching meeting id".to_string()));
            out.push(read.unwrap_or_else(|error| Info {
                id: id.clone(),
                title: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                created_at: 0.0,
                updated_at: 0.0,
                lines: 0,
                pages: 0,
                read_error: Some(error),
            }));
        }
        Ok(())
    }

    /// Newest first. A meeting with a v2 copy is listed once, from that copy.
    pub fn list(&self) -> Result<Vec<Info>> {
        let mut out = vec![];
        Self::scan(&self.current_dir(), &mut out)?;
        let tombstoned: Vec<String> = out.iter().map(|m| m.id.clone()).collect();
        let current = self.current_dir();
        let mut older = vec![];
        Self::scan(&self.legacy_dir(), &mut older)?;
        out.extend(older.into_iter().filter(|m| !tombstoned.contains(&m.id) && !Self::deleted(&current, &m.id)));
        out.sort_by(|a, b| b.updated_at.total_cmp(&a.updated_at));
        Ok(out)
    }

    pub fn load(&self, id: &str) -> Result<Option<Loaded>> {
        valid(id)?;
        let current = self.current_dir();
        if Self::deleted(&current, id) {
            return Ok(None);
        }
        let path = current.join(format!("{id}.json"));
        if path.is_file() {
            let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
            let value: Value = serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
            let revision = value["storageRevision"].as_u64().unwrap_or(0);
            let Value::Object(raw) = value else { bail!("{} is not a meeting", path.display()) };
            return Ok(Some(Loaded { raw, origin: Origin::Current, revision }));
        }
        let legacy = self.legacy_dir();
        let path = legacy.join(format!("{id}.json"));
        if !path.is_file() || Self::deleted(&legacy, id) {
            return Ok(None);
        }
        let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let value: Value = serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        let source_hash = Self::source_hash(&text);
        let Value::Object(raw) = value else { bail!("{} is not a meeting", path.display()) };
        Ok(Some(Loaded { raw, origin: Origin::Legacy { source_hash }, revision: 0 }))
    }

    fn lock(dir: &Path, id: &str) -> Result<File> {
        let lock = OpenOptions::new().create(true).truncate(false).write(true).open(dir.join(format!("{id}.lock")))?;
        lock.lock()?;
        Ok(lock)
    }

    /// Write a meeting and return its new revision. `expected` is the revision it was based on; a
    /// different one on disk means another window saved first, and nothing is written.
    pub fn save(&self, id: &str, mut meeting: Map<String, Value>, origin: &Origin, expected: u64) -> Result<u64> {
        valid(id)?;
        let dir = self.current_dir();
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        // The same OS lock as the older app, so saves from both serialize.
        let _lock = Self::lock(&dir, id)?;
        if Self::deleted(&dir, id) {
            bail!("This meeting has been deleted. Your current edits are still here.");
        }
        let path = dir.join(format!("{id}.json"));
        let existing: Option<Value> = fs::read_to_string(&path).ok().and_then(|text| serde_json::from_str(&text).ok());
        let on_disk = existing.as_ref().and_then(|v| v["storageRevision"].as_u64()).unwrap_or(0);
        if on_disk != expected {
            bail!("This meeting changed in another window. Reopen it before saving; current edits are preserved.");
        }
        if existing.as_ref().and_then(|v| v["version"].as_u64()).unwrap_or(1) > MEETING_VERSION {
            bail!("Cannot overwrite a newer meeting format");
        }
        if let Origin::Legacy { source_hash } = origin {
            let original = fs::read_to_string(self.legacy_dir().join(format!("{id}.json"))).ok();
            if original.is_some_and(|text| &Self::source_hash(&text) != source_hash) {
                bail!("The older copy of this meeting changed after it was opened. Reopen it before saving.");
            }
            meeting.entry("migration").or_insert_with(|| json!({ "sourceHash": source_hash }));
        }
        let revision = expected + 1;
        meeting.insert("version".into(), json!(MEETING_VERSION));
        meeting.insert("storageRevision".into(), json!(revision));
        let text = serde_json::to_string(&meeting)?;
        let tmp = dir.join(format!("{id}.json.tmp"));
        let write = || -> Result<()> {
            let mut file = File::create(&tmp)?;
            file.write_all(text.as_bytes())?;
            // On disk before the rename, so a crash leaves the old file or the new one, never half.
            file.sync_all()?;
            fs::rename(&tmp, &path)?;
            File::open(&dir)?.sync_all()?;
            Ok(())
        };
        write().with_context(|| format!("saving {}", path.display()))?;
        Ok(revision)
    }

    /// The meeting to reopen at startup.
    pub fn last_opened(&self) -> Option<String> {
        fs::read_to_string(self.root.join("gpui-last-meeting")).ok().map(|s| s.trim().to_string()).filter(|s| valid(s).is_ok())
    }

    pub fn remember_opened(&self, id: &str) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        fs::write(self.root.join("gpui-last-meeting"), id)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static NEXT: AtomicU32 = AtomicU32::new(0);

    struct Dir(PathBuf);

    impl Dir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("boardcast-store-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn meeting(id: &str) -> Map<String, Value> {
        let Value::Object(m) = json!({ "id": id, "title": "会", "createdAt": 1, "updatedAt": 2, "lines": [], "future": { "kept": true } }) else { unreachable!() };
        m
    }

    #[test]
    fn saving_and_loading_keeps_unknown_fields_and_counts_revisions() {
        let dir = Dir::new();
        let store = Store::new(dir.0.clone());
        assert_eq!(store.save("a-1", meeting("a-1"), &Origin::New, 0).unwrap(), 1);
        let loaded = store.load("a-1").unwrap().unwrap();
        assert_eq!(loaded.origin, Origin::Current);
        assert_eq!(loaded.revision, 1);
        assert_eq!(loaded.raw["future"]["kept"], true);
        assert_eq!(loaded.raw["version"], 2);
        assert_eq!(store.save("a-1", loaded.raw, &Origin::Current, 1).unwrap(), 2);
    }

    #[test]
    fn a_stale_save_is_refused_and_keeps_the_newer_file() {
        let dir = Dir::new();
        let store = Store::new(dir.0.clone());
        store.save("a-1", meeting("a-1"), &Origin::New, 0).unwrap();
        let mut newer = meeting("a-1");
        newer.insert("title".into(), json!("新"));
        store.save("a-1", newer, &Origin::Current, 1).unwrap();
        let error = store.save("a-1", meeting("a-1"), &Origin::Current, 1).unwrap_err();
        assert!(error.to_string().contains("another window"), "{error}");
        assert_eq!(store.load("a-1").unwrap().unwrap().raw["title"], "新");
    }

    #[test]
    fn a_newer_format_and_a_deleted_meeting_are_never_overwritten() {
        let dir = Dir::new();
        let store = Store::new(dir.0.clone());
        fs::create_dir_all(store.current_dir()).unwrap();
        fs::write(store.current_dir().join("a-1.json"), r#"{"id":"a-1","version":3,"storageRevision":4}"#).unwrap();
        assert!(store.save("a-1", meeting("a-1"), &Origin::Current, 4).is_err());
        assert_eq!(store.load("a-1").unwrap().unwrap().raw["version"], 3);
        fs::write(store.current_dir().join("b-1.deleted"), "").unwrap();
        assert!(store.save("b-1", meeting("b-1"), &Origin::New, 0).is_err());
        assert!(store.load("b-1").unwrap().is_none());
    }

    #[test]
    fn a_legacy_original_is_left_alone_and_hashed_on_first_save() {
        let dir = Dir::new();
        let store = Store::new(dir.0.clone());
        fs::create_dir_all(store.legacy_dir()).unwrap();
        let text = r#"{"version":1,"id":"old-1","title":"旧","updatedAt":5.5,"lines":[{"text":"甲","ms":3}]}"#;
        fs::write(store.legacy_dir().join("old-1.json"), text).unwrap();
        let loaded = store.load("old-1").unwrap().unwrap();
        let Origin::Legacy { source_hash } = &loaded.origin else { panic!("expected legacy") };
        assert_eq!(store.save("old-1", loaded.raw.clone(), &loaded.origin, 0).unwrap(), 1);
        assert_eq!(fs::read_to_string(store.legacy_dir().join("old-1.json")).unwrap(), text);
        let saved = store.load("old-1").unwrap().unwrap();
        assert_eq!(saved.origin, Origin::Current);
        assert_eq!(saved.raw["migration"]["sourceHash"], source_hash.as_str());
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1, "one row, not one per format");
    }

    #[test]
    fn a_legacy_original_changed_since_opening_blocks_the_save() {
        let dir = Dir::new();
        let store = Store::new(dir.0.clone());
        fs::create_dir_all(store.legacy_dir()).unwrap();
        fs::write(store.legacy_dir().join("old-1.json"), r#"{"id":"old-1","title":"旧"}"#).unwrap();
        let loaded = store.load("old-1").unwrap().unwrap();
        fs::write(store.legacy_dir().join("old-1.json"), r#"{"id":"old-1","title":"别处改过"}"#).unwrap();
        assert!(store.save("old-1", loaded.raw, &loaded.origin, 0).is_err());
    }

    #[test]
    fn a_damaged_file_stays_listed_with_its_error_and_ids_cannot_escape() {
        let dir = Dir::new();
        let store = Store::new(dir.0.clone());
        fs::create_dir_all(store.legacy_dir()).unwrap();
        fs::write(store.legacy_dir().join("bad-1.json"), "{ not json").unwrap();
        let listed = store.list().unwrap();
        assert!(listed[0].read_error.is_some());
        assert!(store.load("../../etc/passwd").is_err());
        assert!(store.save("../x", meeting("x"), &Origin::New, 0).is_err());
    }

    #[test]
    fn a_write_that_fails_reports_it_and_leaves_the_old_file() {
        let dir = Dir::new();
        let store = Store::new(dir.0.clone());
        store.save("a-1", meeting("a-1"), &Origin::New, 0).unwrap();
        // A directory where the temporary file must go makes the write fail however it runs.
        fs::create_dir_all(store.current_dir().join("a-1.json.tmp")).unwrap();
        let mut changed = meeting("a-1");
        changed.insert("title".into(), json!("未写入"));
        assert!(store.save("a-1", changed, &Origin::Current, 1).is_err());
        assert_eq!(store.load("a-1").unwrap().unwrap().raw["title"], "会");
    }

    #[test]
    fn ids_are_file_safe_and_differ() {
        let (a, b) = (new_id(), new_id());
        valid(&a).unwrap();
        assert_eq!(a.len(), 20, "{a}");
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert_ne!(new_id(), b);
    }
}
