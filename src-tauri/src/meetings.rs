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
}

fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    let d = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("meetings");
    std::fs::create_dir_all(&d).map_err(|e| format!("建不了会议目录：{e}"))?;
    Ok(d)
}

/// 编号直接当文件名用：只允许字母、数字和横线，杜绝 ../ 之类
fn valid(id: &str) -> Result<(), String> {
    if !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        Ok(())
    } else {
        Err(format!("会议编号不合法：{id}"))
    }
}

/// 图片文件名：内容哈希（十六进制）加扩展名，杜绝 ../ 之类
fn valid_asset(key: &str) -> Result<(), String> {
    let ok = key.split_once('.').is_some_and(|(h, ext)| {
        (16..=64).contains(&h.len())
            && h.chars().all(|c| c.is_ascii_hexdigit())
            && ["jpg", "png", "webp"].contains(&ext)
    });
    if ok {
        Ok(())
    } else {
        Err(format!("图片名不合法：{key}"))
    }
}

fn summary(v: &Value) -> Option<MeetingInfo> {
    Some(MeetingInfo {
        id: v.get("id")?.as_str()?.to_string(),
        title: v
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("未命名会议")
            .to_string(),
        created_at: v.get("createdAt").and_then(Value::as_f64).unwrap_or(0.0),
        updated_at: v.get("updatedAt").and_then(Value::as_f64).unwrap_or(0.0),
        // 句数只算会上说的话，不算手写板上的手动修改
        lines: v.get("lines").and_then(Value::as_array).map_or(0, |ls| {
            ls.iter()
                .filter(|l| !l.get("manual").and_then(Value::as_bool).unwrap_or(false))
                .count()
        }),
    })
}

#[tauri::command]
pub fn meetings_list(app: AppHandle) -> Result<Vec<MeetingInfo>, String> {
    let mut out = vec![];
    for e in std::fs::read_dir(dir(&app)?)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        // 读不了或者坏掉的文件跳过，不让一个文件毁掉整个列表
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if let Some(m) = summary(&v) {
            out.push(m);
        }
    }
    Ok(out)
}

#[tauri::command]
pub fn meeting_load(app: AppHandle, id: String) -> Result<Option<String>, String> {
    valid(&id)?;
    let p = dir(&app)?.join(format!("{id}.json"));
    if !p.is_file() {
        return Ok(None);
    }
    std::fs::read_to_string(&p)
        .map(Some)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn meeting_save(app: AppHandle, id: String, json: String) -> Result<(), String> {
    valid(&id)?;
    // 只存合法的 JSON，免得写进去以后再也读不出来
    serde_json::from_str::<Value>(&json).map_err(|e| format!("会议内容不是合法的 JSON：{e}"))?;
    let d = dir(&app)?;
    let tmp = d.join(format!("{id}.json.tmp"));
    std::fs::write(&tmp, json).map_err(|e| format!("保存会议失败：{e}"))?;
    std::fs::rename(&tmp, d.join(format!("{id}.json"))).map_err(|e| format!("保存会议失败：{e}"))
}

#[tauri::command]
pub fn meeting_delete(app: AppHandle, id: String) -> Result<(), String> {
    valid(&id)?;
    let d = dir(&app)?;
    let p = d.join(format!("{id}.json"));
    if p.is_file() {
        std::fs::remove_file(p).map_err(|e| e.to_string())?;
    }
    // 连同它的图片
    let assets = d.join(&id);
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
) -> Result<(), String> {
    use base64::Engine;
    valid(&id)?;
    valid_asset(&key)?;
    let d = dir(&app)?.join(&id);
    let p = d.join(&key);
    if p.is_file() {
        return Ok(());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .map_err(|e| format!("图片数据不对：{e}"))?;
    std::fs::create_dir_all(&d).map_err(|e| format!("建不了图片目录：{e}"))?;
    let tmp = d.join(format!("{key}.tmp"));
    std::fs::write(&tmp, bytes).map_err(|e| format!("保存图片失败：{e}"))?;
    std::fs::rename(&tmp, &p).map_err(|e| format!("保存图片失败：{e}"))
}

/// 读一张图，返回 base64；没有就是 None
#[tauri::command]
pub fn meeting_asset_load(
    app: AppHandle,
    id: String,
    key: String,
) -> Result<Option<String>, String> {
    use base64::Engine;
    valid(&id)?;
    valid_asset(&key)?;
    let p = dir(&app)?.join(&id).join(&key);
    if !p.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
    Ok(Some(
        base64::engine::general_purpose::STANDARD.encode(bytes),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

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
