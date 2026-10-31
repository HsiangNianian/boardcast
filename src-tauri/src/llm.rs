//! 本地模型：把打包进来的 llama-server 拉起来，前端通过 `llm_status` 拿到地址。
//!
//! - 模型放在 `<应用数据目录>/models/` 下（下载器见 models.rs），或者用环境变量 `BOARDCAST_MODEL` 指定一个 .gguf；
//!   目录里有多个模型时，用 `selected.txt` 里记的那个，没记就按 9B、4B 的顺序挑。
//! - 有独显就只用独显（核显和独显同时存在时，llama.cpp 默认会把模型拆到两张卡上，反而更慢）；
//!   GPU 起不来就退回纯 CPU 再试一次。
//! - 应用退出、崩溃或被强制结束时，子进程一起结束。
//! - 下载完新模型或者切换模型时调用 `restart`。每次启动都有一个代号，旧的那次发现自己过期了就收手，
//!   免得两次启动互相覆盖状态。

use serde::Serialize;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    pub id: String,
    pub name: String,
    pub total_mib: u64,
    pub discrete: bool,
}

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LlmStatus {
    /// idle | starting | ready | no-model | error
    /// idle：远程模型优先，内置模型先不启动，用到（远程不可用、某句要兜底）时再拉起
    pub state: String,
    pub url: Option<String>,
    pub model: Option<String>,
    /// 用了哪块设备，或者纯 CPU
    pub device: Option<String>,
    pub message: Option<String>,
    pub models_dir: Option<String>,
    /// 这台机器上 llama.cpp 能看到的 GPU，前端用来推荐模型
    pub gpus: Vec<Gpu>,
    /// 带着看图组件（导入图片时能读字）
    pub vision: bool,
}

#[derive(Default)]
pub struct Llm {
    status: Mutex<LlmStatus>,
    child: Mutex<Option<Child>>,
    generation: AtomicU64,
}

impl Llm {
    fn set(&self, generation: u64, s: LlmStatus) {
        if self.generation.load(Ordering::SeqCst) == generation {
            *self.status.lock().unwrap() = s;
        }
    }

    fn current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == generation
    }

    /// 应用退出时、重启前调用
    pub fn shutdown(&self) {
        if let Some(mut c) = self.child.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    pub fn gpus(&self) -> Vec<Gpu> {
        self.status.lock().unwrap().gpus.clone()
    }
}

#[tauri::command]
pub fn llm_status(llm: tauri::State<'_, Llm>) -> LlmStatus {
    llm.status.lock().unwrap().clone()
}

#[tauri::command]
pub fn llm_restart(app: AppHandle) {
    restart(&app);
}

pub fn models_dir(app: &AppHandle) -> Option<PathBuf> {
    let d = app.path().app_data_dir().ok()?.join("models");
    let _ = std::fs::create_dir_all(&d);
    Some(d)
}

/// 用户选定的模型文件名记在这里
pub const SELECTED: &str = "selected.txt";

/// 没选定时的优先顺序：高配档、轻量档，其余按文件名
const PREFERRED: &[&str] = &["qwen3.5-9b", "qwen3.5-4b"];

fn is_model(p: &Path) -> bool {
    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    // 分片模型只认第一片，llama-server 会自己找其余的
    name.ends_with(".gguf")
        && !name.contains("mmproj")
        && (!name.contains("-of-") || name.contains("-00001-of-"))
}

fn pick_model(dir: &Path) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("BOARDCAST_MODEL") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(name) = std::fs::read_to_string(dir.join(SELECTED)) {
        let p = dir.join(name.trim());
        if p.is_file() && is_model(&p) {
            return Some(p);
        }
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| is_model(p))
        .collect();
    files.sort_by_key(|p| {
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        let rank = PREFERRED
            .iter()
            .position(|k| name.starts_with(k))
            .unwrap_or(PREFERRED.len());
        (rank, name)
    });
    files.into_iter().next()
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .unwrap_or(18080)
}

fn healthy(port: u16) -> bool {
    let Ok(mut s) =
        TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(300))
    else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_millis(800)));
    if s.write_all(b"GET /health HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut buf = [0u8; 64];
    let n = s.read(&mut buf).unwrap_or(0);
    std::str::from_utf8(&buf[..n])
        .map(|h| h.contains(" 200"))
        .unwrap_or(false)
}

fn command(exe: &Path) -> Command {
    let mut cmd = Command::new(exe);
    if let Some(dir) = exe.parent() {
        // Windows 按当前目录找 dll；Linux 的 RUNPATH 是 $ORIGIN，设不设都行
        cmd.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // 不弹黑色的控制台窗口
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// 解析 `--list-devices`，比如 `Vulkan1: NVIDIA GeForce RTX 3060 Laptop GPU (6144 MiB, 5822 MiB free)`
fn list_devices(exe: &Path) -> Vec<Gpu> {
    let Ok(out) = command(exe)
        .arg("--list-devices")
        .stdin(Stdio::null())
        .output()
    else {
        return vec![];
    };
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    let discrete = [
        "nvidia",
        "geforce",
        "radeon rx",
        "radeon pro",
        " arc",
        "adreno",
    ];
    text.lines()
        .filter_map(|l| {
            let (id, rest) = l.trim().split_once(": ")?;
            if id.is_empty() || id.contains(' ') {
                return None;
            }
            let (name, mem) = rest.rsplit_once(" (").unwrap_or((rest, ""));
            let total_mib = mem
                .split(" MiB")
                .next()
                .and_then(|n| n.trim().parse().ok())
                .unwrap_or(0);
            let lower = name.to_lowercase();
            Some(Gpu {
                id: id.to_string(),
                name: name.to_string(),
                total_mib,
                discrete: discrete.iter().any(|k| lower.contains(k)),
            })
        })
        .collect()
}

fn spawn(
    exe: &Path,
    model: &Path,
    port: u16,
    device: Option<&str>,
    mmproj: Option<&Path>,
    log: &Path,
) -> std::io::Result<Child> {
    let out = File::create(log)?;
    let err = out.try_clone()?;
    let mut cmd = command(exe);
    cmd.args(["-m"])
        .arg(model)
        .args(["--host", "127.0.0.1", "--port", &port.to_string()])
        .args(["-c", "4096", "-np", "1", "--jinja"]);
    // 看图组件：导入图片时读图里的字。没 GPU 时也照样加载，只是读得慢
    if let Some(p) = mmproj {
        cmd.arg("--mmproj").arg(p);
    }
    match device {
        // 不写死 -ngl 99：显存不够放下整个模型时（6 GB 卡跑 9B，或者显存被别的程序占了），
        // 让 llama.cpp 按剩余显存放尽量多的层，其余留在 CPU，而不是整个启动失败退回纯 CPU
        Some(d) => cmd.args(["--device", d, "-ngl", "auto", "--fit", "on", "-fa", "auto"]),
        None => cmd.args(["--device", "none", "-ngl", "0"]),
    };
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        // 父进程（准确说是发起 fork 的那个线程）没了，子进程就收到 SIGKILL。
        // 所以 start() 里的线程在子进程活着的时候不能退出，见那里的看门狗循环。
        unsafe {
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                Ok(())
            });
        }
    }
    let child = cmd.stdin(Stdio::null()).stdout(out).stderr(err).spawn()?;
    #[cfg(windows)]
    kill_on_close(&child);
    Ok(child)
}

/// Windows：把子进程放进一个「句柄关闭就杀掉所有进程」的 Job。
/// Job 的句柄故意不关，我们的进程一退出（包括崩溃）系统就会关掉它，llama-server 随之结束。
#[cfg(windows)]
fn kill_on_close(child: &Child) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        AssignProcessToJobObject(job, child.as_raw_handle() as _);
    }
}

/// 等它就绪；进程提前退出（比如显存不够）或者这次启动已经过期，就返回 false
fn wait_ready(child: &mut Child, port: u16, timeout: Duration, alive: impl Fn() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout && alive() {
        if let Ok(Some(_)) = child.try_wait() {
            return false;
        }
        if healthy(port) {
            return true;
        }
        thread::sleep(Duration::from_millis(250));
    }
    false
}

/// 换了模型之后重新加载；原来在待命就继续待命，只刷新状态
pub fn restart(app: &AppHandle) {
    let llm = app.state::<Llm>();
    let idle = llm.status.lock().unwrap().state == "idle";
    llm.shutdown();
    launch(app.clone(), !idle);
}

/// 拉起内置模型
pub fn start(app: AppHandle) {
    launch(app, true);
}

/// 不拉起，只把 GPU、模型这些信息准备好，状态记成 idle（远程模型优先时）
pub fn stand_by(app: AppHandle) {
    launch(app, false);
}

/// 前端要用内置模型了（远程连不上、某一句要兜底）：还在待命就拉起来，已经在跑就什么都不做
#[tauri::command]
pub fn llm_ensure(app: AppHandle) -> LlmStatus {
    // 在锁里先改成 starting：连着来两次也只拉起一个
    let llm = app.state::<Llm>();
    let was_idle = {
        let mut s = llm.status.lock().unwrap();
        let idle = s.state == "idle";
        if idle {
            s.state = "starting".into();
        }
        idle
    };
    if was_idle {
        start(app.clone());
    }
    app.state::<Llm>().status.lock().unwrap().clone()
}

/// 远程模型启用并连上了：停掉内置模型，把显存和内存让出来
#[tauri::command]
pub fn llm_stop(app: AppHandle) {
    app.state::<Llm>().shutdown();
    stand_by(app);
}

fn launch(app: AppHandle, run: bool) {
    let generation = app.state::<Llm>().generation.fetch_add(1, Ordering::SeqCst) + 1;
    thread::spawn(move || {
        let llm = app.state::<Llm>();
        let models_dir = models_dir(&app);
        let dir_str = models_dir.as_ref().map(|d| d.display().to_string());

        let exe_name = if cfg!(windows) {
            "llama-server.exe"
        } else {
            "llama-server"
        };
        let exe = app
            .path()
            .resource_dir()
            .map(|d| d.join("llama").join(exe_name))
            .ok()
            .filter(|p| p.is_file());
        let gpus = exe.as_deref().map(list_devices).unwrap_or_default();
        let base = LlmStatus {
            models_dir: dir_str,
            gpus: gpus.clone(),
            ..Default::default()
        };

        let Some(model) = models_dir.as_deref().and_then(pick_model) else {
            llm.set(
                generation,
                LlmStatus {
                    state: "no-model".into(),
                    message: Some("No model installed; download one in Settings → Manage models".into()),
                    ..base
                },
            );
            return;
        };
        let model_name = model.file_name().map(|n| n.to_string_lossy().to_string());
        let mmproj = models_dir
            .as_deref()
            .zip(model_name.as_deref())
            .and_then(|(d, f)| crate::models::vision_for(d, f));
        let base = LlmStatus {
            vision: mmproj.is_some(),
            ..base
        };
        if !run {
            llm.set(
                generation,
                LlmStatus {
                    state: "idle".into(),
                    model: model_name,
                    message: Some("Using the remote model; the built-in model will start when needed".into()),
                    ..base
                },
            );
            return;
        }

        let Some(exe) = exe else {
            llm.set(
                generation,
                LlmStatus {
                    state: "error".into(),
                    model: model_name,
                    message: Some("llama-server was not found in the app installation".into()),
                    ..base
                },
            );
            return;
        };

        let log = app
            .path()
            .app_log_dir()
            .map(|d| {
                let _ = std::fs::create_dir_all(&d);
                d.join("llama-server.log")
            })
            .unwrap_or_else(|_| std::env::temp_dir().join("boardcast-llama-server.log"));

        // 先试 GPU（优先独显），不行再试纯 CPU
        let gpu = gpus
            .iter()
            .find(|g| g.discrete)
            .or_else(|| gpus.first())
            .cloned();
        let mut attempts: Vec<Option<Gpu>> = vec![];
        if gpu.is_some() {
            attempts.push(gpu);
        }
        attempts.push(None);

        for dev in attempts {
            if !llm.current(generation) {
                return;
            }
            let port = free_port();
            let device_name = dev
                .as_ref()
                .map(|d| d.name.clone())
                .unwrap_or_else(|| "CPU".into());
            llm.set(
                generation,
                LlmStatus {
                    state: "starting".into(),
                    model: model_name.clone(),
                    device: Some(device_name.clone()),
                    ..base.clone()
                },
            );
            let Ok(mut child) = spawn(
                &exe,
                &model,
                port,
                dev.as_ref().map(|d| d.id.as_str()),
                mmproj.as_deref(),
                &log,
            ) else {
                continue;
            };
            // 大模型第一次加载可能比较久
            if wait_ready(&mut child, port, Duration::from_secs(180), || {
                llm.current(generation)
            }) {
                if !llm.current(generation) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
                *llm.child.lock().unwrap() = Some(child);
                llm.set(
                    generation,
                    LlmStatus {
                        state: "ready".into(),
                        url: Some(format!("http://127.0.0.1:{port}")),
                        model: model_name.clone(),
                        device: Some(device_name),
                        ..base.clone()
                    },
                );
                // 看门狗：这个线程要一直活着（Linux 的 PDEATHSIG 跟着线程走），顺便盯着子进程
                loop {
                    thread::sleep(Duration::from_secs(1));
                    if !llm.current(generation) {
                        return;
                    }
                    let mut guard = llm.child.lock().unwrap();
                    let Some(c) = guard.as_mut() else { return };
                    if let Ok(Some(code)) = c.try_wait() {
                        *guard = None;
                        llm.set(
                            generation,
                            LlmStatus {
                                state: "error".into(),
                                model: model_name,
                                message: Some(format!(
                                    "llama-server exited unexpectedly ({code}); log: {}",
                                    log.display()
                                )),
                                ..base
                            },
                        );
                        return;
                    }
                }
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        llm.set(
            generation,
            LlmStatus {
                state: "error".into(),
                model: model_name,
                message: Some(format!("llama-server failed to start; log: {}", log.display())),
                ..base
            },
        );
    });
}
