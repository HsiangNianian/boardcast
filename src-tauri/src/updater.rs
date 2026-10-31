//! 自动更新（#17）：Tauri 官方的 updater 插件。
//!
//! - 更新清单是 GitHub 上最新正式版里的 `latest.json`（Release 工作流打包时由 tauri-action 生成），
//!   里面有各平台安装包的地址和签名；签名用仓库 secrets 里的私钥签，公钥写在 tauri.conf.json 里。
//! - 前端只调两个命令：`update_check` 看有没有新版本，`update_install` 下载、校验签名、安装、重启。
//!   下载进度通过 `update-progress` 事件推给前端。网页那边不用开 updater 插件的权限。
//! - 预发布版（带 -rc 之类后缀）不会成为 GitHub 的 latest，所以不会推给用户。

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// 检查到的、还没装的更新
#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    /// 更新说明（Release 正文，也就是 CHANGELOG 那一节）
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Clone, Serialize)]
struct Progress {
    done: u64,
    total: Option<u64>,
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let found = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| format!("Update check failed: {e}"))?;
    let info = found.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        current: u.current_version.clone(),
        notes: u.body.clone().filter(|s| !s.trim().is_empty()),
        date: u.date.map(|d| d.date().to_string()),
    });
    *app.state::<Pending>().0.lock().unwrap() = found;
    Ok(info)
}

#[cfg(test)]
mod tests {
    /// tauri.conf.json 里 updater 的配置解析不了，应用一启动就会崩；CI 只打包不启动，这里先检查
    #[test]
    fn updater_config_parses() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let c: tauri_plugin_updater::Config =
            serde_json::from_value(conf["plugins"]["updater"].clone()).expect("updater 配置不对");
        assert_eq!(c.endpoints.len(), 1);
        assert!(c.endpoints[0]
            .as_str()
            .ends_with("/releases/latest/download/latest.json"));
        // 公钥是 base64 包着的 minisign 公钥
        use base64::Engine;
        let key = base64::engine::general_purpose::STANDARD
            .decode(c.pubkey.trim())
            .expect("公钥不是 base64");
        assert!(String::from_utf8_lossy(&key).contains("minisign public key"));
    }
}

/// 下载、校验签名、安装，然后重启。成功的话不会返回
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    let update = app
        .state::<Pending>()
        .0
        .lock()
        .unwrap()
        .take()
        .ok_or("No update ready to install; check for updates first")?;
    let emitter = app.clone();
    let mut done = 0u64;
    let bytes = update
        .download(
            move |chunk, total| {
                done += chunk as u64;
                let _ = emitter.emit("update-progress", Progress { done, total });
            },
            || {},
        )
        .await
        .map_err(|e| format!("Update download failed: {e}"))?;
    // 下好了、签名也对，再把收音和内置模型停掉（Windows 上 llama-server 占着安装目录里的文件，安装器覆盖不了）；
    // 下载失败的话这场会照常开
    crate::voice::voice_stop(app.clone());
    app.state::<crate::llm::Llm>().shutdown();
    update
        .install(bytes)
        .map_err(|e| format!("Update installation failed: {e}"))?;
    app.restart()
}
