//! 自动更新（#17）：Tauri 官方的 updater 插件。
//!
//! - 更新清单是 GitHub 上最新正式版里的 `latest.json`（Release 工作流打包时由 tauri-action 生成），
//!   里面有各平台安装包的地址和签名；签名用仓库 secrets 里的私钥签，公钥写在 tauri.conf.json 里。
//! - `update_download` 下载并校验；前端停止收音、确认保存成功后，`update_install` 安装并重启。
//!   下载进度通过 `update-progress` 事件推给前端。网页那边不用开 updater 插件的权限。
//! - 预发布版（带 -rc 之类后缀）不会成为 GitHub 的 latest，所以不会推给用户。

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// 检查到的、还没装的更新
#[derive(Default)]
pub struct Pending(Mutex<Prepared<Update>>);

// Keep the selected release through download/save/install failures. No mutex is held over I/O.
struct Prepared<T> {
    update: Option<T>,
    bytes: Option<Vec<u8>>,
    busy: bool,
}

impl<T> Default for Prepared<T> {
    fn default() -> Self {
        Self {
            update: None,
            bytes: None,
            busy: false,
        }
    }
}

impl<T: Clone> Prepared<T> {
    fn replace(&mut self, update: Option<T>) {
        // A check must not replace the version being downloaded or awaiting installation.
        if !self.busy && self.bytes.is_none() {
            self.update = update;
        }
    }

    fn download(&mut self) -> Result<Option<T>, String> {
        if self.busy {
            return Err("An update operation is already running".into());
        }
        if self.bytes.is_some() {
            return Ok(None);
        }
        let update = self
            .update
            .clone()
            .ok_or("No update ready; check for updates first")?;
        self.busy = true;
        Ok(Some(update))
    }

    fn downloaded(&mut self, bytes: Option<Vec<u8>>) {
        self.busy = false;
        self.bytes = bytes;
    }

    fn install(&mut self) -> Result<(T, Vec<u8>), String> {
        if self.busy {
            return Err("An update operation is already running".into());
        }
        let update = self
            .update
            .clone()
            .ok_or("No update ready; check for updates first")?;
        let bytes = self
            .bytes
            .take()
            .ok_or("Download and verify the update before installing")?;
        self.busy = true;
        Ok((update, bytes))
    }
}

#[derive(Serialize)]
pub struct UpdateError {
    stage: &'static str,
    message: String,
}

impl UpdateError {
    fn new(stage: &'static str, message: impl ToString) -> Self {
        Self {
            stage,
            message: message.to_string(),
        }
    }
}

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
    let updater = app
        .updater_builder()
        .configure_client(|client| {
            client
                .connect_timeout(std::time::Duration::from_secs(30))
                .read_timeout(std::time::Duration::from_secs(30))
        })
        .build()
        .map_err(|e| e.to_string())?;
    let found = tokio::time::timeout(std::time::Duration::from_secs(30), updater.check())
        .await
        .map_err(|_| "UPDATE_CHECK_TIMEOUT".to_string())?
        .map_err(|e| format!("Update check failed: {e}"))?;
    let pending = app.state::<Pending>();
    let mut pending = pending.0.lock().unwrap();
    pending.replace(found);
    let info = pending.update.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        current: u.current_version.clone(),
        notes: u.body.clone().filter(|s| !s.trim().is_empty()),
        date: u.date.map(|d| d.date().to_string()),
    });
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::Prepared;

    #[test]
    fn failed_download_keeps_the_same_release_retryable() {
        let mut pending = Prepared::default();
        pending.replace(Some("v2"));
        assert_eq!(pending.download().unwrap(), Some("v2"));
        assert!(pending.download().is_err());
        pending.replace(Some("v3"));
        pending.downloaded(None);
        assert_eq!(pending.download().unwrap(), Some("v2"));
        assert!(pending.install().is_err());
    }

    #[test]
    fn verified_bytes_survive_checks_and_install_failure() {
        let mut pending = Prepared::default();
        pending.replace(Some("v2"));
        pending.download().unwrap();
        pending.downloaded(Some(vec![1, 2, 3]));
        pending.replace(Some("v3"));
        assert_eq!(pending.download().unwrap(), None);
        let (version, bytes) = pending.install().unwrap();
        assert_eq!(version, "v2");
        assert!(pending.install().is_err());
        pending.downloaded(Some(bytes));
        assert_eq!(pending.install().unwrap(), ("v2", vec![1, 2, 3]));
    }

    #[test]
    fn install_requires_verified_download() {
        let mut pending = Prepared::default();
        pending.replace(Some("v2"));
        assert!(pending.install().is_err());
        assert_eq!(pending.download().unwrap(), Some("v2"));
    }

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

/// Download and verify without stopping the meeting. Failed downloads remain retryable.
#[tauri::command]
pub async fn update_download(app: AppHandle) -> Result<(), UpdateError> {
    let update = app
        .state::<Pending>()
        .0
        .lock()
        .unwrap()
        .download()
        .map_err(|e| UpdateError::new("download", e))?;
    let Some(update) = update else {
        return Ok(());
    };
    let emitter = app.clone();
    let mut done = 0u64;
    let result = update
        .download(
            move |chunk, total| {
                done += chunk as u64;
                let _ = emitter.emit("update-progress", Progress { done, total });
            },
            || {},
        )
        .await;
    let state = app.state::<Pending>();
    let mut pending = state.0.lock().unwrap();
    match result {
        Ok(bytes) => {
            pending.downloaded(Some(bytes));
            Ok(())
        }
        Err(error) => {
            pending.downloaded(None);
            use tauri_plugin_updater::Error;
            let stage = match &error {
                Error::Minisign(_)
                | Error::Base64(_)
                | Error::SignatureUtf8(_)
                | Error::SignedVersionMismatch { .. }
                | Error::MissingSignedVersion => "signature",
                _ => "download",
            };
            Err(UpdateError::new(stage, error))
        }
    }
}

/// Called only after the frontend has stopped capture and flushed the current revision.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), UpdateError> {
    let (update, bytes) = app
        .state::<Pending>()
        .0
        .lock()
        .unwrap()
        .install()
        .map_err(|e| UpdateError::new("install", e))?;
    // Windows installers need the model process to release files in the application directory.
    crate::voice::unload(&app);
    app.state::<crate::llm::Llm>().shutdown();
    if let Err(error) = update.install(&bytes) {
        app.state::<Pending>()
            .0
            .lock()
            .unwrap()
            .downloaded(Some(bytes));
        return Err(UpdateError::new("install", error));
    }
    app.restart()
}
