//! 模型下载器（#3）。
//!
//! - 清单写死在这里：每个模型的文件名、字节数、sha256（以 Hugging Face 上登记的 LFS oid 为准）和几个下载源。
//! - 下载源按顺序试：ModelScope（直连，不走系统代理，国内实测快几十倍）→ hf-mirror → huggingface.co。
//!   某个源出错就换下一个，已经下到的部分接着用（同一个文件，从哪下都一样，最后统一校验）。
//! - 断点续传：未完成的部分存在 `<文件名>.part`，取消或断网后再点下载会接着下。
//! - 一边下一边算 sha256，续传时先把已有的部分过一遍；对不上就删掉重下。
//! - 下完写进 `selected.txt` 并重启 llama-server。
//! - 进度通过 `model-progress` 事件推给前端。

use crate::llm;
use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Source {
    name: &'static str,
    url: &'static str,
    /// true：不走系统代理（国内源走代理反而更慢）
    direct: bool,
}

struct Entry {
    id: &'static str,
    label: &'static str,
    /// 给界面看的一句话
    note: &'static str,
    file: &'static str,
    size: u64,
    sha256: &'static str,
    /// 建议的最小显存（MiB），用来做推荐
    vram_mib: u64,
    sources: &'static [Source],
}

const CATALOG: &[Entry] = &[
    Entry {
        id: "qwen3.5-4b",
        label: "Qwen3.5-4B",
        note: "轻量档：轻薄本、核显、纯 CPU 也能用",
        file: "Qwen3.5-4B-Q4_K_M.gguf",
        size: 2_740_937_888,
        sha256: "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4",
        vram_mib: 0,
        sources: &[
            Source {
                name: "ModelScope",
                url: "https://modelscope.cn/models/unsloth/Qwen3.5-4B-GGUF/resolve/master/Qwen3.5-4B-Q4_K_M.gguf",
                direct: true,
            },
            Source {
                name: "hf-mirror",
                url: "https://hf-mirror.com/unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf",
                direct: false,
            },
            Source {
                name: "Hugging Face",
                url: "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf",
                direct: false,
            },
        ],
    },
    Entry {
        id: "qwen3.5-9b",
        label: "Qwen3.5-9B",
        note: "高配档：更准，建议有 6 GB 以上显存的独显",
        file: "Qwen3.5-9B-Q4_K_M.gguf",
        size: 5_680_522_464,
        sha256: "03b74727a860a56338e042c4420bb3f04b2fec5734175f4cb9fa853daf52b7e8",
        vram_mib: 6000,
        sources: &[
            Source {
                name: "ModelScope",
                url: "https://modelscope.cn/models/unsloth/Qwen3.5-9B-GGUF/resolve/master/Qwen3.5-9B-Q4_K_M.gguf",
                direct: true,
            },
            Source {
                name: "hf-mirror",
                url: "https://hf-mirror.com/unsloth/Qwen3.5-9B-GGUF/resolve/main/Qwen3.5-9B-Q4_K_M.gguf",
                direct: false,
            },
            Source {
                name: "Hugging Face",
                url: "https://huggingface.co/unsloth/Qwen3.5-9B-GGUF/resolve/main/Qwen3.5-9B-Q4_K_M.gguf",
                direct: false,
            },
        ],
    },
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    id: &'static str,
    label: &'static str,
    note: &'static str,
    file: &'static str,
    size: u64,
    installed: bool,
    /// 已经下了多少（没下完的 .part）
    partial: u64,
    selected: bool,
    recommended: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    models: Vec<ModelInfo>,
    dir: Option<String>,
    downloading: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    id: String,
    /// downloading | verifying | done | error | canceled
    state: &'static str,
    downloaded: u64,
    total: u64,
    /// 字节每秒
    speed: u64,
    source: Option<String>,
    message: Option<String>,
}

#[derive(Default)]
pub struct Downloads {
    current: Mutex<Option<(String, Arc<AtomicBool>)>>,
}

fn entry(id: &str) -> Result<&'static Entry, String> {
    CATALOG
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("没有这个模型：{id}"))
}

fn selected(dir: &Path) -> Option<String> {
    std::fs::read_to_string(dir.join(llm::SELECTED))
        .ok()
        .map(|s| s.trim().to_string())
}

/// 推荐：独显显存够得着的最大那一档；没有独显就推荐最小的（清单按从小到大排）
fn recommended(app: &AppHandle) -> &'static str {
    let vram = app
        .state::<llm::Llm>()
        .gpus()
        .iter()
        .filter(|g| g.discrete)
        .map(|g| g.total_mib)
        .max()
        .unwrap_or(0);
    CATALOG
        .iter()
        .rev()
        .find(|e| e.vram_mib <= vram)
        .unwrap_or(&CATALOG[0])
        .id
}

#[tauri::command]
pub fn models_list(app: AppHandle, downloads: tauri::State<'_, Downloads>) -> Catalog {
    let dir = llm::models_dir(&app);
    let sel = dir.as_deref().and_then(selected);
    let rec = recommended(&app);
    let models = CATALOG
        .iter()
        .map(|e| {
            let path = dir.as_ref().map(|d| d.join(e.file));
            let installed = path.as_ref().is_some_and(|p| p.is_file());
            let partial = path
                .as_ref()
                .and_then(|p| std::fs::metadata(part_of(p)).ok())
                .map(|m| m.len())
                .unwrap_or(0);
            ModelInfo {
                id: e.id,
                label: e.label,
                note: e.note,
                file: e.file,
                size: e.size,
                installed,
                partial,
                selected: installed && sel.as_deref() == Some(e.file),
                recommended: e.id == rec,
            }
        })
        .collect();
    Catalog {
        models,
        dir: dir.map(|d| d.display().to_string()),
        downloading: downloads
            .current
            .lock()
            .unwrap()
            .as_ref()
            .map(|c| c.0.clone()),
    }
}

fn part_of(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

#[tauri::command]
pub fn model_download(
    app: AppHandle,
    downloads: tauri::State<'_, Downloads>,
    id: String,
) -> Result<(), String> {
    let e = entry(&id)?;
    let dir = llm::models_dir(&app).ok_or("找不到应用数据目录")?;
    let cancel = {
        let mut cur = downloads.current.lock().unwrap();
        if let Some((other, _)) = cur.as_ref() {
            return Err(format!("{other} 正在下载，等它下完或者先取消"));
        }
        let flag = Arc::new(AtomicBool::new(false));
        *cur = Some((id.clone(), flag.clone()));
        flag
    };
    tauri::async_runtime::spawn(async move {
        let result = download(&app, e, &dir, &cancel).await;
        app.state::<Downloads>().current.lock().unwrap().take();
        let path = dir.join(e.file);
        match result {
            Ok(()) => {
                let _ = std::fs::write(dir.join(llm::SELECTED), e.file);
                emit(&app, e, "done", e.size, 0, None, None);
                llm::restart(&app);
            }
            Err(DlError::Canceled) => {
                let have = std::fs::metadata(part_of(&path))
                    .map(|m| m.len())
                    .unwrap_or(0);
                emit(&app, e, "canceled", have, 0, None, None);
            }
            Err(DlError::Failed(msg)) => {
                let have = std::fs::metadata(part_of(&path))
                    .map(|m| m.len())
                    .unwrap_or(0);
                emit(&app, e, "error", have, 0, None, Some(msg));
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn model_cancel(downloads: tauri::State<'_, Downloads>) {
    if let Some((_, flag)) = downloads.current.lock().unwrap().as_ref() {
        flag.store(true, Ordering::SeqCst);
    }
}

#[tauri::command]
pub fn model_select(app: AppHandle, id: String) -> Result<(), String> {
    let e = entry(&id)?;
    let dir = llm::models_dir(&app).ok_or("找不到应用数据目录")?;
    if !dir.join(e.file).is_file() {
        return Err(format!("{} 还没下载", e.label));
    }
    std::fs::write(dir.join(llm::SELECTED), e.file).map_err(|err| err.to_string())?;
    llm::restart(&app);
    Ok(())
}

#[tauri::command]
pub fn model_delete(
    app: AppHandle,
    downloads: tauri::State<'_, Downloads>,
    id: String,
) -> Result<(), String> {
    let e = entry(&id)?;
    if downloads
        .current
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|c| c.0 == id)
    {
        return Err("正在下载，先取消".into());
    }
    let dir = llm::models_dir(&app).ok_or("找不到应用数据目录")?;
    let path = dir.join(e.file);
    let was_selected = selected(&dir).as_deref() == Some(e.file);
    // 先停掉 llama-server：它可能正开着这个文件，Windows 上被占用的文件删不掉。删完再按剩下的模型重新拉起
    app.state::<llm::Llm>().shutdown();
    let _ = std::fs::remove_file(part_of(&path));
    if path.exists() {
        std::fs::remove_file(&path).map_err(|err| err.to_string())?;
    }
    if was_selected {
        let _ = std::fs::remove_file(dir.join(llm::SELECTED));
    }
    llm::restart(&app);
    Ok(())
}

enum DlError {
    Canceled,
    Failed(String),
}

fn emit(
    app: &AppHandle,
    e: &Entry,
    state: &'static str,
    downloaded: u64,
    speed: u64,
    source: Option<&str>,
    message: Option<String>,
) {
    let _ = app.emit(
        "model-progress",
        Progress {
            id: e.id.to_string(),
            state,
            downloaded,
            total: e.size,
            speed,
            source: source.map(str::to_string),
            message,
        },
    );
}

fn client(direct: bool) -> Result<reqwest::Client, String> {
    let mut b = reqwest::Client::builder()
        .user_agent(concat!("boardcast/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        // 30 秒一个字节都没来就当断了，换下一个源
        .read_timeout(Duration::from_secs(30));
    if direct {
        b = b.no_proxy();
    }
    b.build().map_err(|err| err.to_string())
}

/// 进度回调：(状态, 已下载字节, 速度, 下载源, 说明)
type Report<'a> = dyn FnMut(&'static str, u64, u64, Option<&str>, Option<String>) + Send + 'a;

async fn download(
    app: &AppHandle,
    e: &Entry,
    dir: &Path,
    cancel: &AtomicBool,
) -> Result<(), DlError> {
    fetch(
        e,
        dir,
        cancel,
        &mut |state, have, speed, source, message| {
            emit(app, e, state, have, speed, source, message)
        },
    )
    .await
}

/// 真正的下载逻辑，不依赖 Tauri，方便单独测试
async fn fetch(
    e: &Entry,
    dir: &Path,
    cancel: &AtomicBool,
    report: &mut Report<'_>,
) -> Result<(), DlError> {
    let path = dir.join(e.file);
    if path.is_file() {
        return Ok(());
    }
    let part = part_of(&path);

    // 续传：先把已经下到的部分算进哈希
    let mut hasher = Sha256::new();
    let mut have: u64 = 0;
    if let Ok(mut f) = tokio::fs::File::open(&part).await {
        report("verifying", 0, 0, None, Some("核对已下载的部分".into()));
        let mut buf = vec![0u8; 8 << 20];
        loop {
            let n = f
                .read(&mut buf)
                .await
                .map_err(|err| DlError::Failed(err.to_string()))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            have += n as u64;
            if cancel.load(Ordering::SeqCst) {
                return Err(DlError::Canceled);
            }
        }
    }
    if have > e.size {
        // 不可能比完整文件还大，说明是坏的
        let _ = tokio::fs::remove_file(&part).await;
        hasher = Sha256::new();
        have = 0;
    }

    let mut last_error = String::from("所有下载源都失败了");
    for src in e.sources {
        if have >= e.size {
            break;
        }
        let Ok(http) = client(src.direct) else {
            continue;
        };
        let mut req = http.get(src.url);
        if have > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let resp = match req.send().await {
            Ok(r) => r,
            Err(err) => {
                last_error = format!("{}：{err}", src.name);
                continue;
            }
        };
        let status = resp.status();
        if have > 0 && status == reqwest::StatusCode::OK {
            // 这个源不支持续传，只能从头来
            hasher = Sha256::new();
            have = 0;
            let _ = tokio::fs::remove_file(&part).await;
        } else if !status.is_success() {
            last_error = format!("{}：HTTP {status}", src.name);
            continue;
        }
        let mut file = match tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&part)
            .await
        {
            Ok(f) => f,
            Err(err) => return Err(DlError::Failed(format!("写不进模型目录：{err}"))),
        };
        let mut stream = resp.bytes_stream();
        let mut last_emit = Instant::now();
        let mut window_start = Instant::now();
        let mut window_bytes: u64 = 0;
        let mut speed: u64 = 0;
        let mut broken = false;
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::SeqCst) {
                let _ = file.flush().await;
                return Err(DlError::Canceled);
            }
            let chunk = match chunk {
                Ok(c) => c,
                Err(err) => {
                    last_error = format!("{}：{err}", src.name);
                    broken = true;
                    break;
                }
            };
            if let Err(err) = file.write_all(&chunk).await {
                // 多半是磁盘满了
                return Err(DlError::Failed(format!("写入失败：{err}")));
            }
            hasher.update(&chunk);
            have += chunk.len() as u64;
            window_bytes += chunk.len() as u64;
            let w = window_start.elapsed();
            if w >= Duration::from_secs(1) {
                speed = (window_bytes as f64 / w.as_secs_f64()) as u64;
                window_start = Instant::now();
                window_bytes = 0;
            }
            if last_emit.elapsed() >= Duration::from_millis(250) {
                last_emit = Instant::now();
                report("downloading", have, speed, Some(src.name), None);
            }
        }
        let _ = file.flush().await;
        if !broken && have >= e.size {
            break;
        }
    }
    if have < e.size {
        return Err(DlError::Failed(last_error));
    }

    report("verifying", have, 0, None, Some("校验文件".into()));
    let got = format!("{:x}", hasher.finalize());
    if got != e.sha256 {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(DlError::Failed(format!(
            "校验失败（sha256 {got}），已删除，请重新下载"
        )));
    }
    tokio::fs::rename(&part, &path)
        .await
        .map_err(|err| DlError::Failed(err.to_string()))?;
    Ok(())
}

#[cfg(test)]
#[path = "models_tests.rs"]
mod tests;
