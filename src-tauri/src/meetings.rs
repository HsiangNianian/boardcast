//! 会议的保存和读取：应用数据目录下的 meetings/，一场会议一个 JSON 文件（内容由前端组织）。
//! 写文件时先写临时文件再改名，写到一半崩溃也不会留下坏掉的会议。
//! 导入的 PDF 页、图片单独存在 meetings/<会议编号>/ 下（文件名是内容的哈希），JSON 里只记引用，
//! 这样自动保存时不用每次把几 MB 的图片重写一遍。

use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingInfo {
    id: String,
    title: String,
    created_at: f64,
    updated_at: f64,
    lines: usize,
    pages: usize,
    unfinished: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    read_error: Option<String>,
}

fn namespace(version: Option<u32>) -> Result<&'static str, String> {
    match version.unwrap_or(1) {
        1 => Ok("meetings"),
        2 => Ok("meetings/v2"),
        _ => Err("Unsupported meeting storage version".into()),
    }
}

fn dir(app: &AppHandle, storage_version: Option<u32>) -> Result<PathBuf, String> {
    let namespace = namespace(storage_version)?;
    let d = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join(namespace);
    std::fs::create_dir_all(&d).map_err(|e| format!("Couldn't create the meetings folder: {e}"))?;
    Ok(d)
}

/// 编号直接当文件名用：只允许字母、数字和横线，杜绝 ../ 之类
fn valid(id: &str) -> Result<(), String> {
    if !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        Ok(())
    } else {
        Err(format!("Invalid meeting ID: {id}"))
    }
}

/// 图片文件名：内容哈希（十六进制）加扩展名，杜绝 ../ 之类
fn valid_asset(key: &str) -> Result<(), String> {
    let ok = key.split_once('.').is_some_and(|(h, ext)| {
        (16..=64).contains(&h.len())
            && h.chars().all(|c| c.is_ascii_hexdigit())
            && ["jpg", "png", "webp", "gif", "svg"].contains(&ext)
    });
    if ok {
        Ok(())
    } else {
        Err(format!("Invalid image name: {key}"))
    }
}

/// v1 hashes the complete canonical data URL, not the raw file bytes.
fn check_asset_hash(key: &str, bytes: &[u8]) -> Result<(), String> {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    valid_asset(key)?;
    let (hash, ext) = key.split_once('.').unwrap();
    let mime = match ext {
        "jpg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        _ => unreachable!(),
    };
    let url = format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    );
    let actual = format!("{:x}", Sha256::digest(url.as_bytes()));
    if actual[..hash.len()].eq_ignore_ascii_case(hash) {
        Ok(())
    } else {
        Err(format!(
            "ASSET_HASH_MISMATCH: {key}. The original image is preserved."
        ))
    }
}

fn meeting_lock(d: &std::path::Path, id: &str) -> Result<std::fs::File, String> {
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(d.join(format!("{id}.lock")))
        .map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    Ok(lock)
}

fn is_deleted(d: &std::path::Path, id: &str) -> Result<bool, String> {
    d.join(format!("{id}.deleted"))
        .try_exists()
        .map_err(|e| e.to_string())
}

fn check_not_deleted(d: &std::path::Path, id: &str) -> Result<(), String> {
    if is_deleted(d, id)? {
        Err("This meeting has been deleted. Your current edits are still here.".into())
    } else {
        Ok(())
    }
}

fn save_asset_at(
    namespace: &std::path::Path,
    id: &str,
    key: &str,
    bytes: &[u8],
) -> Result<(), String> {
    // Share the record/delete lock so a late image cannot recreate deleted content.
    let _lock = meeting_lock(namespace, id)?;
    check_not_deleted(namespace, id)?;
    check_asset_hash(key, bytes)?;
    let d = namespace.join(id);
    let p = d.join(key);
    if p.is_file() {
        let existing = std::fs::read(&p).map_err(|e| e.to_string())?;
        check_asset_hash(key, &existing)?;
        return if existing == bytes {
            Ok(())
        } else {
            Err(format!(
                "Image {key} already exists with different content. The original is preserved."
            ))
        };
    }
    std::fs::create_dir_all(&d).map_err(|e| format!("Couldn't create the image folder: {e}"))?;
    let tmp = d.join(format!("{key}.tmp"));
    std::fs::write(&tmp, bytes).map_err(|e| format!("Couldn't save the image: {e}"))?;
    std::fs::rename(&tmp, &p).map_err(|e| format!("Couldn't save the image: {e}"))
}

fn summary(v: &Value) -> Option<MeetingInfo> {
    Some(MeetingInfo {
        id: v.get("id")?.as_str()?.to_string(),
        title: v
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Untitled meeting")
            .to_string(),
        created_at: v.get("createdAt").and_then(Value::as_f64).unwrap_or(0.0),
        updated_at: v.get("updatedAt").and_then(Value::as_f64).unwrap_or(0.0),
        // 句数只算会上说的话，不算手写板上的手动修改
        lines: v.get("lines").and_then(Value::as_array).map_or(0, |ls| {
            ls.iter()
                .filter(|l| !l.get("manual").and_then(Value::as_bool).unwrap_or(false))
                .count()
        }),
        pages: v
            .get("pages")
            .and_then(Value::as_array)
            .map_or(1, |pages| pages.len().max(1)),
        unfinished: v.get("lines").and_then(Value::as_array).map_or(0, |lines| {
            lines
                .iter()
                .filter(|line| {
                    !line.get("manual").and_then(Value::as_bool).unwrap_or(false)
                        && (line.get("failed").and_then(Value::as_bool).unwrap_or(false)
                            || line.get("ms").is_none_or(Value::is_null))
                })
                .count()
        }),
        read_error: None,
    })
}

#[tauri::command]
pub fn meetings_list(
    app: AppHandle,
    storage_version: Option<u32>,
    strict: Option<bool>,
) -> Result<Vec<MeetingInfo>, String> {
    list_at(&dir(&app, storage_version)?, strict.unwrap_or(false))
}

fn list_at(d: &std::path::Path, strict: bool) -> Result<Vec<MeetingInfo>, String> {
    let mut out = vec![];
    for e in std::fs::read_dir(d).map_err(|e| e.to_string())? {
        let p = e.map_err(|error| error.to_string())?.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let id = p.file_stem().unwrap_or_default().to_string_lossy();
        if is_deleted(d, &id)? {
            continue;
        }
        // The library stays browseable; exporting all samples must not silently omit records.
        let record = (|| {
            let text = std::fs::read_to_string(&p)
                .map_err(|e| format!("Could not read meeting {}: {e}", p.display()))?;
            let v = serde_json::from_str::<Value>(&text)
                .map_err(|e| format!("Could not read meeting {}: {e}", p.display()))?;
            let meeting = summary(&v)
                .filter(|m| m.id == id && valid(&m.id).is_ok())
                .ok_or_else(|| format!("Meeting has no matching valid ID: {}", p.display()))?;
            Ok(meeting)
        })();
        match record {
            Ok(m) => out.push(m),
            Err(error) if strict => return Err(error),
            Err(error) => out.push(MeetingInfo {
                id: id.to_string(),
                title: p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                created_at: 0.0,
                updated_at: 0.0,
                lines: 0,
                pages: 0,
                unfinished: 0,
                read_error: Some(error),
            }),
        }
    }
    Ok(out)
}

#[tauri::command]
pub fn meeting_load(
    app: AppHandle,
    id: String,
    storage_version: Option<u32>,
) -> Result<Option<String>, String> {
    valid(&id)?;
    let d = dir(&app, storage_version)?;
    if is_deleted(&d, &id)? {
        return Ok(None);
    }
    let p = d.join(format!("{id}.json"));
    if !p.is_file() {
        return Ok(None);
    }
    std::fs::read_to_string(&p)
        .map(Some)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn meeting_save(
    app: AppHandle,
    id: String,
    json: String,
    storage_version: Option<u32>,
    expected_revision: Option<u64>,
) -> Result<(), String> {
    valid(&id)?;
    save_at(&dir(&app, storage_version)?, &id, &json, expected_revision)
}

fn save_at(
    d: &std::path::Path,
    id: &str,
    json: &str,
    expected_revision: Option<u64>,
) -> Result<(), String> {
    // 只存合法的 JSON，免得写进去以后再也读不出来
    serde_json::from_str::<Value>(json)
        .map_err(|e| format!("Meeting content is not valid JSON: {e}"))?;
    // OS lock serializes the compare-and-replace across processes as well as windows.
    let _lock = meeting_lock(d, id)?;
    check_not_deleted(d, id)?;
    if let Some(expected) = expected_revision {
        let existing = std::fs::read_to_string(d.join(format!("{id}.json")))
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok());
        check_revision(existing.as_ref(), expected)?;
    }
    // Never let an older writer flatten unknown future fields.
    let path = d.join(format!("{id}.json"));
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(existing) = serde_json::from_str::<Value>(&text) {
            let incoming = serde_json::from_str::<Value>(json).map_err(|e| e.to_string())?;
            if existing.get("version").and_then(Value::as_u64).unwrap_or(1)
                > incoming.get("version").and_then(Value::as_u64).unwrap_or(1)
            {
                return Err("Cannot overwrite a newer meeting format".into());
            }
        }
    }
    let tmp = d.join(format!("{id}.json.tmp"));
    std::fs::write(&tmp, json).map_err(|e| format!("Couldn't save the meeting: {e}"))?;
    std::fs::rename(&tmp, d.join(format!("{id}.json")))
        .map_err(|e| format!("Couldn't save the meeting: {e}"))
}

fn check_revision(existing: Option<&Value>, expected: u64) -> Result<(), String> {
    let revision = existing
        .and_then(|v| v.get("storageRevision"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if revision != expected {
        Err("This meeting changed in another window. Reopen it before saving; current edits are preserved.".into())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub fn meeting_delete(
    app: AppHandle,
    id: String,
    storage_version: Option<u32>,
) -> Result<(), String> {
    valid(&id)?;
    delete_at(&dir(&app, storage_version)?, &id)
}

fn delete_at(d: &std::path::Path, id: &str) -> Result<(), String> {
    let _lock = meeting_lock(d, id)?;
    let marker = d.join(format!("{id}.deleted"));
    let already_deleted = is_deleted(d, id)?;
    // Persist only an empty marker before removing content; all writers hold this same lock.
    if !already_deleted {
        std::fs::File::create_new(&marker).map_err(|e| e.to_string())?;
    }
    let p = d.join(format!("{id}.json"));
    if let Err(error) = std::fs::remove_file(p) {
        if error.kind() != std::io::ErrorKind::NotFound {
            if !already_deleted {
                std::fs::remove_file(&marker).map_err(|rollback| format!("Couldn't delete the meeting: {error}; couldn't restore its availability: {rollback}"))?;
            }
            return Err(error.to_string());
        }
    }
    if let Err(error) = std::fs::remove_file(d.join(format!("{id}.json.tmp"))) {
        if error.kind() != std::io::ErrorKind::NotFound {
            return Err(error.to_string());
        }
    }
    // 连同它的图片
    let assets = d.join(id);
    if assets.is_dir() {
        std::fs::remove_dir_all(assets).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 存一张图（base64）。同名的已经有了就不再写：名字就是内容的哈希
#[tauri::command]
pub fn meeting_asset_save(
    app: AppHandle,
    id: String,
    key: String,
    data: String,
    storage_version: Option<u32>,
) -> Result<(), String> {
    use base64::Engine;
    valid(&id)?;
    valid_asset(&key)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .map_err(|e| format!("Invalid image data: {e}"))?;
    save_asset_at(&dir(&app, storage_version)?, &id, &key, &bytes)
}

/// 读一张图，返回 base64；没有就是 None
#[tauri::command]
pub fn meeting_asset_load(
    app: AppHandle,
    id: String,
    key: String,
    storage_version: Option<u32>,
) -> Result<Option<String>, String> {
    use base64::Engine;
    valid(&id)?;
    valid_asset(&key)?;
    let p = dir(&app, storage_version)?.join(&id).join(&key);
    if !p.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
    check_asset_hash(&key, &bytes)?;
    Ok(Some(
        base64::engine::general_purpose::STANDARD.encode(bytes),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_listing_reports_unreadable_records_without_blocking_the_library() {
        let folder =
            std::env::temp_dir().join(format!("boardcast-sample-list-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let valid = r#"{"id":"valid","title":"Valid meeting"}"#;
        std::fs::write(folder.join("valid.json"), valid).unwrap();
        let bad = folder.join("bad.json");
        std::fs::create_dir(&bad).unwrap();
        assert_eq!(list_at(&folder, false).unwrap().len(), 2);
        assert!(list_at(&folder, true).err().unwrap().contains("bad.json"));
        std::fs::remove_dir(&bad).unwrap();
        for text in [
            "{truncated",
            "{}",
            r#"{"id":"different"}"#,
            r#"{"id":"../bad"}"#,
        ] {
            std::fs::write(&bad, text).unwrap();
            let listed = list_at(&folder, false).unwrap();
            assert_eq!(listed.len(), 2);
            let failed = listed.iter().find(|m| m.id == "bad").unwrap();
            assert!(failed.read_error.as_ref().unwrap().contains("bad.json"));
            assert_eq!(failed.title, "bad.json");
            assert_eq!(std::fs::read_to_string(&bad).unwrap(), text);
            let good = listed.iter().find(|m| m.id == "valid").unwrap();
            assert!(serde_json::to_value(good)
                .unwrap()
                .get("readError")
                .is_none());
            assert!(list_at(&folder, true).err().unwrap().contains("bad.json"));
        }
        std::fs::write(folder.join("bad.deleted"), "deleted").unwrap();
        assert_eq!(list_at(&folder, true).unwrap().len(), 1);
        assert_eq!(
            std::fs::read_to_string(folder.join("valid.json")).unwrap(),
            valid
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn library_summary_counts_pages_and_unfinished_speech_only() {
        let meeting = serde_json::json!({"id": "counts", "pages": [{}, {}], "lines": [
            {"text": "pending"}, {"text": "failed", "ms": 0, "failed": true},
            {"text": "done", "ms": 8}, {"text": "edit", "manual": true}
        ]});
        let info = summary(&meeting).unwrap();
        assert_eq!((info.lines, info.pages, info.unfinished), (3, 2, 2));
        assert_eq!(
            summary(&serde_json::json!({"id": "empty"})).unwrap().pages,
            1
        );
    }

    #[test]
    fn stale_writer_cannot_overwrite_newer_revision() {
        assert!(check_revision(None, 0).is_ok());
        let saved = serde_json::json!({"storageRevision": 2});
        assert!(check_revision(Some(&saved), 2).is_ok());
        assert!(check_revision(Some(&saved), 1).is_err());
    }

    #[test]
    fn v2_save_preserves_original_and_rejects_future_overwrite() {
        let temp = std::env::temp_dir().join(format!(
            "boardcast-migration-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let legacy = temp.join(namespace(None).unwrap());
        let current = temp.join(namespace(Some(2)).unwrap());
        std::fs::create_dir_all(legacy.join("m1")).unwrap();
        std::fs::create_dir_all(current.join("m1")).unwrap();
        let original = r#"{"version":1,"id":"m1","graph":{"nodes":[]}}"#;
        std::fs::write(legacy.join("m1.json"), original).unwrap();
        std::fs::write(legacy.join("m1/image.png"), b"original asset").unwrap();
        save_at(
            &current,
            "m1",
            r#"{"version":2,"id":"m1","scene":{"elements":[]}}"#,
            Some(0),
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(legacy.join("m1.json")).unwrap(),
            original
        );
        assert_eq!(
            std::fs::read(legacy.join("m1/image.png")).unwrap(),
            b"original asset"
        );
        assert!(save_at(&current, "m1", original, Some(0)).is_err());
        assert!(namespace(Some(99)).is_err());
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn asset_names_are_content_hashes() {
        assert!(valid_asset("0123456789abcdef0123456789abcdef.jpg").is_ok());
        assert!(valid_asset("0123456789ABCDEF.png").is_ok());
        for bad in [
            "../x.jpg",
            "0123456789abcdef.exe",
            "abc.jpg",
            "0123456789abcdef/../../a.jpg",
            "0123456789abcdef",
            "0123456789abcdeg.jpg",
        ] {
            assert!(valid_asset(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn asset_hash_matches_the_v1_data_url_and_rejects_changed_bytes() {
        // Decoding is checked by the frontend image decoder; hashes authenticate exact bytes.
        assert!(check_asset_hash("4c353aab651f847ff789986e238a6f2f.png", b"not a PNG").is_ok());
        assert!(
            check_asset_hash("4c353aab651f847ff789986e238a6f2f.png", b"changed")
                .unwrap_err()
                .starts_with("ASSET_HASH_MISMATCH:")
        );
        assert!(check_asset_hash("0123456789abcdef0123456789abcdef.png", b"not a PNG").is_err());
    }

    #[test]
    fn asset_save_checks_existing_bytes_and_preserves_a_damaged_original() {
        use base64::Engine;
        use sha2::{Digest, Sha256};
        let dir = std::env::temp_dir().join(format!(
            "boardcast-asset-integrity-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let bytes = br#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>"#;
        let url = format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        let key = format!(
            "{}.svg",
            &format!("{:x}", Sha256::digest(url.as_bytes()))[..32]
        );
        std::fs::create_dir_all(&dir).unwrap();
        save_asset_at(&dir, "m1", &key, bytes).unwrap();
        save_asset_at(&dir, "m1", &key, bytes).unwrap();
        assert_eq!(std::fs::read(dir.join("m1").join(&key)).unwrap(), bytes);
        assert!(save_asset_at(&dir, "m1", &key, b"different incoming data").is_err());
        assert_eq!(std::fs::read(dir.join("m1").join(&key)).unwrap(), bytes);
        std::fs::write(dir.join("m1").join(&key), b"damaged existing data").unwrap();
        assert!(save_asset_at(&dir, "m1", &key, bytes)
            .unwrap_err()
            .starts_with("ASSET_HASH_MISMATCH:"));
        assert_eq!(
            std::fs::read(dir.join("m1").join(&key)).unwrap(),
            b"damaged existing data"
        );
        assert!(!dir.join("m1").join(format!("{key}.tmp")).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn deletion_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "boardcast-delete-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn deletion_fences_late_records_and_images_without_retaining_content() {
        let root = deletion_dir("late");
        let json = r#"{"version":2,"id":"m1","title":"private content"}"#;
        let key = "4c353aab651f847ff789986e238a6f2f.png";
        for version in [1, 2] {
            let dir = root.join(namespace(Some(version)).unwrap());
            std::fs::create_dir_all(&dir).unwrap();
            save_at(&dir, "m1", json, Some(0)).unwrap();
            save_asset_at(&dir, "m1", key, b"not a PNG").unwrap();
            std::fs::write(dir.join("m1.json.tmp"), "interrupted private write").unwrap();
            delete_at(&dir, "m1").unwrap();
            delete_at(&dir, "m1").unwrap();
            for expected in [Some(0), None] {
                assert!(save_at(&dir, "m1", json, expected)
                    .unwrap_err()
                    .contains("deleted"));
            }
            assert!(save_asset_at(&dir, "m1", key, b"not a PNG")
                .unwrap_err()
                .contains("deleted"));
            assert_eq!(std::fs::read(dir.join("m1.deleted")).unwrap(), b"");
            assert!(!dir.join("m1.json").exists());
            assert!(!dir.join("m1.json.tmp").exists());
            assert!(!dir.join("m1").exists());
            save_at(&dir, "other", r#"{"id":"other"}"#, Some(0)).unwrap();
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_delete_record_and_image_writes_share_the_filesystem_lock() {
        let dir = deletion_dir("race");
        for round in 0..12 {
            let id = format!("m{round}");
            let json = format!(r#"{{"id":"{id}","storageRevision":0}}"#);
            save_at(&dir, &id, &json, Some(0)).unwrap();
            let lock = meeting_lock(&dir, &id).unwrap();
            let barrier = std::sync::Barrier::new(4);
            let (send, receive) = std::sync::mpsc::channel();
            std::thread::scope(|scope| {
                for operation in 0..3 {
                    let send = send.clone();
                    let (dir, id, json, barrier) = (&dir, &id, &json, &barrier);
                    scope.spawn(move || {
                        barrier.wait();
                        let result = match operation {
                            0 => delete_at(dir, id),
                            1 => save_at(dir, id, json, Some(0)),
                            _ => save_asset_at(
                                dir,
                                id,
                                "4c353aab651f847ff789986e238a6f2f.png",
                                b"not a PNG",
                            ),
                        };
                        send.send((operation, result)).unwrap();
                    });
                }
                barrier.wait();
                assert!(matches!(
                    receive.recv_timeout(std::time::Duration::from_millis(20)),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                ));
                drop(lock);
            });
            for _ in 0..3 {
                let (operation, result) = receive.recv().unwrap();
                if operation == 0 {
                    result.unwrap();
                } else if let Err(error) = result {
                    assert!(error.contains("deleted"), "{error}");
                }
            }
            assert!(is_deleted(&dir, &id).unwrap());
            assert!(!dir.join(format!("{id}.json")).exists());
            assert!(!dir.join(&id).exists());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn deletion_failure_restores_availability_or_can_retry_remaining_cleanup() {
        let dir = deletion_dir("failure");
        // An undeletable record path must not leave a new deletion marker behind.
        std::fs::create_dir(dir.join("m1.json")).unwrap();
        std::fs::write(dir.join("m1.json/original"), "keep").unwrap();
        assert!(delete_at(&dir, "m1").is_err());
        assert!(!is_deleted(&dir, "m1").unwrap());
        assert_eq!(
            std::fs::read(dir.join("m1.json/original")).unwrap(),
            b"keep"
        );
        std::fs::remove_dir_all(dir.join("m1.json")).unwrap();
        save_at(&dir, "m1", r#"{"id":"m1"}"#, Some(0)).unwrap();
        std::fs::create_dir(dir.join("m1.json.tmp")).unwrap();
        save_asset_at(
            &dir,
            "m1",
            "4c353aab651f847ff789986e238a6f2f.png",
            b"not a PNG",
        )
        .unwrap();
        assert!(delete_at(&dir, "m1").is_err());
        assert!(is_deleted(&dir, "m1").unwrap());
        assert!(save_at(&dir, "m1", r#"{"id":"m1"}"#, Some(0)).is_err());
        std::fs::remove_dir(dir.join("m1.json.tmp")).unwrap();
        delete_at(&dir, "m1").unwrap();
        assert!(!dir.join("m1").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn deletion_fences_independent_processes() {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};
        const JSON: &str = r#"{"version":2,"id":"m1","storageRevision":0}"#;
        const KEY: &str = "4c353aab651f847ff789986e238a6f2f.png";
        if let Ok(role) = std::env::var("BOARDCAST_DELETE_TEST_ROLE") {
            let dir = PathBuf::from(std::env::var_os("BOARDCAST_DELETE_TEST_DIR").unwrap());
            println!("delete-worker-ready:{role}");
            let result = match role.as_str() {
                "delete" => delete_at(&dir, "m1"),
                "save" => save_at(&dir, "m1", JSON, Some(0)),
                "image" => save_asset_at(&dir, "m1", KEY, b"not a PNG"),
                _ => panic!("Unexpected deletion test worker"),
            };
            println!("delete-worker-result:{role}:{result:?}");
            if role == "delete" {
                result.unwrap();
            } else if let Err(error) = result {
                assert!(error.contains("deleted"), "{error}");
            }
            return;
        }
        let root = deletion_dir("processes");
        for version in [1, 2] {
            let dir = root.join(namespace(Some(version)).unwrap());
            std::fs::create_dir_all(&dir).unwrap();
            save_at(&dir, "m1", JSON, Some(0)).unwrap();
            let lock = meeting_lock(&dir, "m1").unwrap();
            let mut children = vec![];
            for role in ["save", "delete", "image"] {
                let mut child = Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "meetings::tests::deletion_fences_independent_processes",
                        "--nocapture",
                    ])
                    .env("BOARDCAST_DELETE_TEST_ROLE", role)
                    .env("BOARDCAST_DELETE_TEST_DIR", &dir)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                let mut output = BufReader::new(child.stdout.take().unwrap());
                let ready = format!("delete-worker-ready:{role}");
                loop {
                    let mut line = String::new();
                    assert!(
                        output.read_line(&mut line).unwrap() > 0,
                        "worker stopped before lock contention"
                    );
                    if line.trim() == ready {
                        break;
                    }
                }
                assert!(child.try_wait().unwrap().is_none());
                children.push((child, output));
            }
            drop(lock);
            for (mut child, mut output) in children {
                let mut text = String::new();
                std::io::Read::read_to_string(&mut output, &mut text).unwrap();
                assert!(child.wait().unwrap().success(), "{text}");
                println!("namespace {version}: {text}");
            }
            assert!(is_deleted(&dir, "m1").unwrap());
            assert!(!dir.join("m1.json").exists());
            assert!(!dir.join("m1").exists());
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ids_cannot_escape_the_meetings_folder() {
        assert!(valid("20261030-103201-ab12").is_ok());
        assert!(valid("../secrets").is_err());
        assert!(valid("a/b").is_err());
        assert!(valid("").is_err());
        assert!(valid(&"x".repeat(65)).is_err());
    }

    #[test]
    fn summary_reads_the_fields_the_list_needs() {
        let v: Value = serde_json::json!({
            "id": "m1", "title": "登录流程", "createdAt": 1.0, "updatedAt": 2.0,
            "lines": [{"text": "a"}, {"text": "b"}, {"text": "删掉了「甲」", "manual": true}], "graph": {}
        });
        let s = summary(&v).unwrap();
        assert_eq!(
            (s.id.as_str(), s.title.as_str(), s.lines),
            ("m1", "登录流程", 2)
        );
        assert!(summary(&serde_json::json!({"title": "没有编号"})).is_none());
    }
}
