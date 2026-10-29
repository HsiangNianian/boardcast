//! 本地模型：把打包进来的 llama-server 拉起来，前端通过 `llm_status` 拿到地址。
//!
//! - 模型放在 `<应用数据目录>/models/` 下，或者用环境变量 `BOARDCAST_MODEL` 指定一个 .gguf；
//!   下载器（#3）做好之前，需要手动把模型放进去。
//! - 有独显就只用独显（核显和独显同时存在时，llama.cpp 默认会把模型拆到两张卡上，反而更慢）；
//!   GPU 起不来就退回纯 CPU 再试一次。
//! - 应用退出时把子进程一起关掉。

use serde::Serialize;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LlmStatus {
    /// starting | ready | no-model | error
    pub state: String,
    pub url: Option<String>,
    pub model: Option<String>,
    /// 用了哪块设备，或者纯 CPU
    pub device: Option<String>,
    pub message: Option<String>,
    pub models_dir: Option<String>,
}

#[derive(Default)]
pub struct Llm {
    status: Mutex<LlmStatus>,
    child: Mutex<Option<Child>>,
}

impl Llm {
    fn set(&self, s: LlmStatus) {
        *self.status.lock().unwrap() = s;
    }

    /// 应用退出时调用
    pub fn shutdown(&self) {
        if let Some(mut c) = self.child.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

#[tauri::command]
pub fn llm_status(llm: tauri::State<'_, Llm>) -> LlmStatus {
    llm.status.lock().unwrap().clone()
}

/// 模型的优先顺序：高配档、轻量档，其余按文件名
const PREFERRED: &[&str] = &["qwen3.5-9b", "qwen3.5-4b"];

fn pick_model(dir: &Path) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("BOARDCAST_MODEL") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
            // 分片模型只认第一片，llama-server 会自己找其余的
            name.ends_with(".gguf") && !name.contains("mmproj") && (!name.contains("-of-") || name.contains("-00001-of-"))
        })
        .collect();
    files.sort_by_key(|p| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
        let rank = PREFERRED.iter().position(|k| name.starts_with(k)).unwrap_or(PREFERRED.len());
        (rank, name)
    });
    files.into_iter().next()
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map(|a| a.port()).unwrap_or(18080)
}

fn healthy(port: u16) -> bool {
    let Ok(mut s) = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(300)) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_millis(800)));
    if s.write_all(b"GET /health HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n").is_err() {
        return false;
    }
    let mut buf = [0u8; 64];
    let n = s.read(&mut buf).unwrap_or(0);
    std::str::from_utf8(&buf[..n]).map(|h| h.contains(" 200")).unwrap_or(false)
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

/// 从 `--list-devices` 里挑一块独显；挑不到就用第一块 GPU；一块都没有就是纯 CPU
fn pick_device(exe: &Path) -> Option<(String, String)> {
    let out = command(exe).arg("--list-devices").stdin(Stdio::null()).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    let devices: Vec<(String, String)> = text
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let (id, rest) = l.split_once(": ")?;
            if id.contains(' ') || id.is_empty() {
                return None;
            }
            Some((id.to_string(), rest.to_string()))
        })
        .collect();
    let discrete = ["nvidia", "geforce", "radeon rx", "radeon pro", " arc", "adreno"];
    devices
        .iter()
        .find(|(_, name)| {
            let n = name.to_lowercase();
            discrete.iter().any(|k| n.contains(k))
        })
        .or_else(|| devices.first())
        .cloned()
}

fn spawn(exe: &Path, model: &Path, port: u16, device: Option<&str>, log: &Path) -> std::io::Result<Child> {
    let out = File::create(log)?;
    let err = out.try_clone()?;
    let mut cmd = command(exe);
    cmd.args(["-m"])
        .arg(model)
        .args(["--host", "127.0.0.1", "--port", &port.to_string()])
        .args(["-c", "4096", "-np", "1", "--jinja"]);
    match device {
        Some(d) => cmd.args(["--device", d, "-ngl", "99", "-fa", "auto"]),
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
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
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

/// 等它就绪；进程提前退出（比如显存不够）就返回 false
fn wait_ready(child: &mut Child, port: u16, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
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

pub fn start(app: AppHandle) {
    thread::spawn(move || {
        let llm = app.state::<Llm>();
        let models_dir = app.path().app_data_dir().map(|d| d.join("models")).ok();
        if let Some(d) = &models_dir {
            let _ = std::fs::create_dir_all(d);
        }
        let dir_str = models_dir.as_ref().map(|d| d.display().to_string());
        let base = LlmStatus { models_dir: dir_str.clone(), ..Default::default() };

        let Some(model) = models_dir.as_deref().and_then(pick_model) else {
            llm.set(LlmStatus {
                state: "no-model".into(),
                message: Some("还没有模型：把 .gguf 文件放进模型目录后重启应用".into()),
                ..base
            });
            return;
        };
        let model_name = model.file_name().map(|n| n.to_string_lossy().to_string());

        let exe_name = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
        let exe = app.path().resource_dir().map(|d| d.join("llama").join(exe_name));
        let Some(exe) = exe.ok().filter(|p| p.is_file()) else {
            llm.set(LlmStatus {
                state: "error".into(),
                model: model_name,
                message: Some("安装包里没有找到 llama-server".into()),
                ..base
            });
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

        // 先试 GPU，不行再试纯 CPU
        let gpu = pick_device(&exe);
        let mut attempts: Vec<Option<(String, String)>> = vec![];
        if gpu.is_some() {
            attempts.push(gpu);
        }
        attempts.push(None);

        for dev in attempts {
            let port = free_port();
            llm.set(LlmStatus {
                state: "starting".into(),
                model: model_name.clone(),
                device: Some(dev.as_ref().map(|d| d.1.clone()).unwrap_or_else(|| "CPU".into())),
                ..base.clone()
            });
            let Ok(mut child) = spawn(&exe, &model, port, dev.as_ref().map(|d| d.0.as_str()), &log) else {
                continue;
            };
            // 大模型第一次加载可能比较久
            if wait_ready(&mut child, port, Duration::from_secs(180)) {
                *llm.child.lock().unwrap() = Some(child);
                llm.set(LlmStatus {
                    state: "ready".into(),
                    url: Some(format!("http://127.0.0.1:{port}")),
                    model: model_name.clone(),
                    device: Some(dev.map(|d| d.1).unwrap_or_else(|| "CPU".into())),
                    ..base.clone()
                });
                // 看门狗：这个线程要一直活着（Linux 的 PDEATHSIG 跟着线程走），顺便盯着子进程
                loop {
                    thread::sleep(Duration::from_secs(1));
                    let mut guard = llm.child.lock().unwrap();
                    let Some(c) = guard.as_mut() else { return };
                    if let Ok(Some(code)) = c.try_wait() {
                        *guard = None;
                        llm.set(LlmStatus {
                            state: "error".into(),
                            model: model_name,
                            message: Some(format!("llama-server 意外退出（{code}），日志在 {}", log.display())),
                            ..base
                        });
                        return;
                    }
                }
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        llm.set(LlmStatus {
            state: "error".into(),
            model: model_name,
            message: Some(format!("llama-server 没能启动，日志在 {}", log.display())),
            ..base
        });
    });
}
