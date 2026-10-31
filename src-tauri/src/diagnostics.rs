//! 诊断信息：系统、架构和 llama-server 日志的末尾。前端再把内容过滤一遍，摆给用户看过才存。

use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};
use tauri::{AppHandle, Manager};

/// 日志只取最后这么多字节
const LOG_TAIL: u64 = 64 * 1024;

#[derive(Serialize)]
pub struct DiagnosticsInfo {
    os: String,
    arch: String,
    log: Option<String>,
}

fn tail(path: &std::path::Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(LOG_TAIL))).ok()?;
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[tauri::command]
pub fn diagnostics_info(app: AppHandle) -> DiagnosticsInfo {
    DiagnosticsInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        log: app
            .path()
            .app_log_dir()
            .ok()
            .and_then(|d| tail(&d.join("llama-server.log"))),
    }
}
