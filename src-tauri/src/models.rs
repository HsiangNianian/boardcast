//! 模型下载器（#3）。
//!
//! - 清单写死在这里。一个条目可以有几个文件（比如语音模型 = 识别模型 + 词表 + 断句模型），
//!   每个文件都写明字节数和 sha256（以 Hugging Face 上登记的为准；非 LFS 小文件是从两个源各下一份算出来核对过的）。
//! - 下载源按顺序试：ModelScope（直连，不走系统代理，国内实测快几十倍）→ hf-mirror → huggingface.co。
//!   某个源出错就换下一个，已经下到的部分接着用（同一个文件，从哪下都一样，最后统一校验）。
//! - 断点续传：未完成的部分存在 `<文件名>.part`，取消或断网后再点下载会接着下。
//! - 一边下一边算 sha256，续传时先把已有的部分过一遍；对不上就删掉重下。
//! - 下载前先看磁盘剩余空间，不够就直接说清楚，不等写到一半才报错。
//! - 大模型下完写进 `selected.txt` 并重启 llama-server；语音模型下完通知语音引擎重新加载。
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

struct FileSpec {
    /// 相对模型目录的路径
    name: &'static str,
    size: u64,
    sha256: &'static str,
    sources: &'static [Source],
}

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// 给 llama-server 用的大模型，同一时间只用一个
    Llm,
    /// 语音识别用的一组小模型
    Voice,
}

struct Entry {
    id: &'static str,
    label: &'static str,
    /// 给界面看的一句话
    note: &'static str,
    kind: Kind,
    files: &'static [FileSpec],
    /// 建议的最小显存（MiB），用来做推荐
    vram_mib: u64,
}

impl Entry {
    fn size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
    /// 大模型交给 llama-server 的那个文件
    fn main_file(&self) -> &'static str {
        self.files[0].name
    }
}

/// 同一个文件的三个源：ModelScope、hf-mirror、Hugging Face
macro_rules! sources {
    ($ms:literal, $hf:literal) => {
        &[
            Source {
                name: "ModelScope",
                url: concat!("https://modelscope.cn/models/", $ms),
                direct: true,
            },
            Source {
                name: "hf-mirror",
                url: concat!("https://hf-mirror.com/", $hf),
                direct: false,
            },
            Source {
                name: "Hugging Face",
                url: concat!("https://huggingface.co/", $hf),
                direct: false,
            },
        ]
    };
}

const CATALOG: &[Entry] = &[
    Entry {
        id: "qwen3.5-4b",
        label: "Qwen3.5-4B",
        note: "轻量档：轻薄本、核显、纯 CPU 也能用",
        kind: Kind::Llm,
        vram_mib: 0,
        files: &[FileSpec {
            name: "Qwen3.5-4B-Q4_K_M.gguf",
            size: 2_740_937_888,
            sha256: "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4",
            sources: sources!(
                "unsloth/Qwen3.5-4B-GGUF/resolve/master/Qwen3.5-4B-Q4_K_M.gguf",
                "unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf"
            ),
        }],
    },
    Entry {
        id: "qwen3.5-9b",
        label: "Qwen3.5-9B",
        note: "高配档：更准，建议有 6 GB 以上显存的独显",
        kind: Kind::Llm,
        vram_mib: 6000,
        files: &[FileSpec {
            name: "Qwen3.5-9B-Q4_K_M.gguf",
            size: 5_680_522_464,
            sha256: "03b74727a860a56338e042c4420bb3f04b2fec5734175f4cb9fa853daf52b7e8",
            sources: sources!(
                "unsloth/Qwen3.5-9B-GGUF/resolve/master/Qwen3.5-9B-Q4_K_M.gguf",
                "unsloth/Qwen3.5-9B-GGUF/resolve/main/Qwen3.5-9B-Q4_K_M.gguf"
            ),
        }],
    },
    Entry {
        id: "voice",
        label: "语音识别",
        note: "SenseVoice（中英日韩粤，带标点）+ Silero 断句，说话时用",
        kind: Kind::Voice,
        vram_mib: 0,
        files: &[
            FileSpec {
                name: "voice/sense-voice/model.int8.onnx",
                size: 239_233_841,
                sha256: "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51",
                sources: sources!(
                    "pengzhendong/sherpa-onnx-sense-voice-zh-en-ja-ko-yue/resolve/master/model.int8.onnx",
                    "csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/model.int8.onnx"
                ),
            },
            FileSpec {
                name: "voice/sense-voice/tokens.txt",
                size: 315_894,
                sha256: "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc",
                sources: sources!(
                    "pengzhendong/sherpa-onnx-sense-voice-zh-en-ja-ko-yue/resolve/master/tokens.txt",
                    "csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/tokens.txt"
                ),
            },
            FileSpec {
                name: "voice/silero_vad.onnx",
                size: 1_807_522,
                sha256: "a35ebf52fd3ce5f1469b2a36158dba761bc47b973ea3382b3186ca15b1f5af28",
                sources: sources!(
                    "pengzhendong/silero-vad/resolve/master/v4/silero_vad.onnx",
                    "csukuangfj/vad/resolve/main/silero_vad.onnx"
                ),
            },
        ],
    },
];

/// 语音模型各个文件的位置，给 voice.rs 用
pub mod voice_files {
    pub const SENSE_VOICE: &str = "voice/sense-voice/model.int8.onnx";
    pub const TOKENS: &str = "voice/sense-voice/tokens.txt";
    pub const VAD: &str = "voice/silero_vad.onnx";
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    id: &'static str,
    label: &'static str,
    note: &'static str,
    kind: Kind,
    size: u64,
    installed: bool,
    /// 已经下了多少（下完的文件 + 没下完的 .part）
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
    /// 模型目录所在磁盘的剩余空间
    free_bytes: Option<u64>,
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

/// 推荐：独显显存够得着的最大那一档大模型；没有独显就推荐最小的（清单按从小到大排）
fn recommended(app: &AppHandle) -> &'static str {
    let vram = app
        .state::<llm::Llm>()
        .gpus()
        .iter()
        .filter(|g| g.discrete)
        .map(|g| g.total_mib)
        .max()
        .unwrap_or(0);
    let llms = || CATALOG.iter().filter(|e| e.kind == Kind::Llm);
    llms()
        .rfind(|e| e.vram_mib <= vram)
        .or_else(|| llms().next())
        .map(|e| e.id)
        .unwrap_or("")
}

fn part_of(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

fn file_size(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

/// 这个条目已经在磁盘上的字节数（下完的文件算全长，没下完的算 .part）
fn on_disk(e: &Entry, dir: &Path) -> u64 {
    e.files
        .iter()
        .map(|f| {
            let p = dir.join(f.name);
            if p.is_file() {
                f.size
            } else {
                file_size(&part_of(&p))
            }
        })
        .sum()
}

pub fn installed(id: &str, dir: &Path) -> bool {
    entry(id).is_ok_and(|e| e.files.iter().all(|f| dir.join(f.name).is_file()))
}

#[tauri::command]
pub fn models_list(app: AppHandle, downloads: tauri::State<'_, Downloads>) -> Catalog {
    let dir = llm::models_dir(&app);
    let sel = dir.as_deref().and_then(selected);
    let rec = recommended(&app);
    let models = CATALOG
        .iter()
        .map(|e| {
            let installed = dir.as_deref().is_some_and(|d| installed(e.id, d));
            ModelInfo {
                id: e.id,
                label: e.label,
                note: e.note,
                kind: e.kind,
                size: e.size(),
                installed,
                partial: dir.as_deref().map(|d| on_disk(e, d)).unwrap_or(0),
                selected: e.kind == Kind::Llm && installed && sel.as_deref() == Some(e.main_file()),
                recommended: e.id == rec,
            }
        })
        .collect();
    Catalog {
        models,
        free_bytes: dir.as_deref().and_then(|d| fs4::available_space(d).ok()),
        dir: dir.map(|d| d.display().to_string()),
        downloading: downloads
            .current
            .lock()
            .unwrap()
            .as_ref()
            .map(|c| c.0.clone()),
    }
}

/// 留一点余量，别把盘写到一个字节都不剩
const SPACE_MARGIN: u64 = 300_000_000;

/// 还需要多少空间；不够就返回给人看的提示
fn check_space(e: &Entry, dir: &Path, free: u64) -> Result<(), String> {
    let need = e.size().saturating_sub(on_disk(e, dir));
    if need + SPACE_MARGIN > free {
        let gb = |n: u64| n as f64 / 1e9;
        return Err(format!(
            "磁盘空间不够：{} 还需要 {:.1} GB（另外留 {:.1} GB 余量），模型目录所在的盘只剩 {:.1} GB",
            e.label,
            gb(need),
            gb(SPACE_MARGIN),
            gb(free)
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn model_download(
    app: AppHandle,
    downloads: tauri::State<'_, Downloads>,
    id: String,
) -> Result<(), String> {
    let e = entry(&id)?;
    let dir = llm::models_dir(&app).ok_or("找不到应用数据目录")?;
    let free = fs4::available_space(&dir).map_err(|err| format!("读不到磁盘剩余空间：{err}"))?;
    check_space(e, &dir, free)?;
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
        match result {
            Ok(()) => {
                emit(&app, e, "done", e.size(), 0, None, None);
                match e.kind {
                    Kind::Llm => {
                        let _ = std::fs::write(dir.join(llm::SELECTED), e.main_file());
                        llm::restart(&app);
                    }
                    Kind::Voice => crate::voice::reload(&app),
                }
            }
            Err(DlError::Canceled) => emit(&app, e, "canceled", on_disk(e, &dir), 0, None, None),
            Err(DlError::Failed(msg)) => {
                emit(&app, e, "error", on_disk(e, &dir), 0, None, Some(msg))
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
    if e.kind != Kind::Llm {
        return Err(format!("{} 不是可以切换的大模型", e.label));
    }
    if !installed(e.id, &dir) {
        return Err(format!("{} 还没下载", e.label));
    }
    std::fs::write(dir.join(llm::SELECTED), e.main_file()).map_err(|err| err.to_string())?;
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
    // 先停掉正在用这些文件的东西：Windows 上被占用的文件删不掉。删完再按剩下的重新拉起
    match e.kind {
        Kind::Llm => app.state::<llm::Llm>().shutdown(),
        Kind::Voice => crate::voice::unload(&app),
    }
    for f in e.files {
        let p = dir.join(f.name);
        let _ = std::fs::remove_file(part_of(&p));
        if p.exists() {
            std::fs::remove_file(&p).map_err(|err| err.to_string())?;
        }
    }
    match e.kind {
        Kind::Llm => {
            if selected(&dir).as_deref() == Some(e.main_file()) {
                let _ = std::fs::remove_file(dir.join(llm::SELECTED));
            }
            llm::restart(&app);
        }
        Kind::Voice => crate::voice::reload(&app),
    }
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
            total: e.size(),
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

/// 进度回调：(状态, 这个文件已下载的字节, 速度, 下载源, 说明)
type Report<'a> = dyn FnMut(&'static str, u64, u64, Option<&str>, Option<String>) + Send + 'a;

/// 一个条目的所有文件依次下；进度按整个条目汇总
async fn download(
    app: &AppHandle,
    e: &Entry,
    dir: &Path,
    cancel: &AtomicBool,
) -> Result<(), DlError> {
    let mut before: u64 = 0;
    for f in e.files {
        let base = before;
        fetch(
            f,
            dir,
            cancel,
            &mut |state, have, speed, source, message| {
                emit(app, e, state, base + have, speed, source, message)
            },
        )
        .await?;
        before += f.size;
    }
    Ok(())
}

/// 下载一个文件。真正的下载逻辑，不依赖 Tauri，方便单独测试
async fn fetch(
    f: &FileSpec,
    dir: &Path,
    cancel: &AtomicBool,
    report: &mut Report<'_>,
) -> Result<(), DlError> {
    let path = dir.join(f.name);
    if path.is_file() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|err| DlError::Failed(format!("建不了目录：{err}")))?;
    }
    let part = part_of(&path);

    // 续传：先把已经下到的部分算进哈希
    let mut hasher = Sha256::new();
    let mut have: u64 = 0;
    if let Ok(mut file) = tokio::fs::File::open(&part).await {
        report("verifying", 0, 0, None, Some("核对已下载的部分".into()));
        let mut buf = vec![0u8; 8 << 20];
        loop {
            let n = file
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
    if have > f.size {
        // 不可能比完整文件还大，说明是坏的
        let _ = tokio::fs::remove_file(&part).await;
        hasher = Sha256::new();
        have = 0;
    }

    let mut last_error = String::from("所有下载源都失败了");
    for src in f.sources {
        if have >= f.size {
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
            Ok(fh) => fh,
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
                // 下载前已经查过空间，到这里多半是别的程序把盘写满了
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
        if !broken && have >= f.size {
            break;
        }
    }
    if have < f.size {
        return Err(DlError::Failed(last_error));
    }

    report("verifying", have, 0, None, Some("校验文件".into()));
    let got = format!("{:x}", hasher.finalize());
    if got != f.sha256 {
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
