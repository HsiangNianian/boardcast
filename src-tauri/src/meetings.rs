//! 会议的保存和读取：应用数据目录下的 meetings/，一场会议一个 JSON 文件（内容由前端组织）。
//! 写文件时先写临时文件再改名，写到一半崩溃也不会留下坏掉的会议。

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
    let p = dir(&app)?.join(format!("{id}.json"));
    if p.is_file() {
        std::fs::remove_file(p).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
