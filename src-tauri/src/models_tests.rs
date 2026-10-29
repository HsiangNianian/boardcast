//! 用本地的假 HTTP 服务测下载逻辑，不依赖外网：
//! 源报错换下一个、下到一半断线接着下、续传带 Range、校验失败删文件、取消后保留进度。

use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Clone, Copy)]
enum Mode {
    /// 直接回 500
    Fail,
    /// 只给前一半，然后断开
    Half,
    /// 正常服务，支持 Range
    Good,
}

fn data() -> Vec<u8> {
    // 3 MB 的确定性数据
    (0..3_000_000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect()
}

/// 起一个假服务，返回地址；`ranges` 记下收到过的 Range 请求头
async fn serve(mode: Mode, ranges: Arc<Mutex<Vec<String>>>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        let body = data();
        loop {
            let Ok((mut s, _)) = l.accept().await else { return };
            let body = body.clone();
            let ranges = ranges.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_lowercase();
                let start = req
                    .lines()
                    .find_map(|l| l.trim().strip_prefix("range: bytes="))
                    .map(|r| {
                        ranges.lock().unwrap().push(r.to_string());
                        r.trim_end_matches('-').parse::<usize>().unwrap_or(0)
                    })
                    .unwrap_or(0);
                match mode {
                    Mode::Fail => {
                        let _ = s.write_all(b"HTTP/1.1 500 Oops\r\ncontent-length: 0\r\n\r\n").await;
                    }
                    Mode::Half => {
                        let head = format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n", body.len());
                        let _ = s.write_all(head.as_bytes()).await;
                        let _ = s.write_all(&body[..body.len() / 2]).await;
                        // 剩下的不发，直接断开
                    }
                    Mode::Good => {
                        let rest = &body[start..];
                        let head = if start > 0 {
                            format!(
                                "HTTP/1.1 206 Partial Content\r\ncontent-length: {}\r\ncontent-range: bytes {start}-{}/{}\r\n\r\n",
                                rest.len(),
                                body.len() - 1,
                                body.len()
                            )
                        } else {
                            format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n", rest.len())
                        };
                        let _ = s.write_all(head.as_bytes()).await;
                        let _ = s.write_all(rest).await;
                    }
                }
                let _ = s.shutdown().await;
            });
        }
    });
    format!("http://{addr}/model.gguf")
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn entry(urls: Vec<String>, sha256: &'static str) -> &'static Entry {
    let sources: Vec<Source> = urls
        .into_iter()
        .enumerate()
        .map(|(i, u)| Source { name: leak(format!("源{i}")), url: leak(u), direct: true })
        .collect();
    Box::leak(Box::new(Entry {
        id: "test",
        label: "test",
        note: "",
        file: "model.gguf",
        size: 3_000_000,
        sha256,
        vram_mib: 0,
        sources: Box::leak(sources.into_boxed_slice()),
    }))
}

fn sha_of_data() -> &'static str {
    leak(format!("{:x}", Sha256::digest(data())))
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("boardcast-dl-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn quiet() -> impl FnMut(&'static str, u64, u64, Option<&str>, Option<String>) + Send {
    |_, _, _, _, _| {}
}

/// 真的从清单里的源下载轻量档模型（2.7 GB），默认跳过：
///   cargo test --manifest-path src-tauri/Cargo.toml --lib -- --ignored --nocapture
#[tokio::test]
#[ignore]
async fn real_download_of_the_small_model() {
    let e = &CATALOG[0];
    let dir = tmp("real");
    let start = std::time::Instant::now();
    let mut last = 0u64;
    let r = fetch(e, &dir, &AtomicBool::new(false), &mut |state, have, speed, source, _| {
        if have >= last + 500_000_000 || state != "downloading" {
            last = have;
            eprintln!("{state} {:.2} GB {:.1} MB/s {}", have as f64 / 1e9, speed as f64 / 1e6, source.unwrap_or(""));
        }
    })
    .await;
    assert!(r.is_ok(), "download failed");
    eprintln!("done in {:.0}s", start.elapsed().as_secs_f64());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn falls_back_and_resumes_across_sources() {
    let ranges = Arc::new(Mutex::new(vec![]));
    let urls = vec![
        serve(Mode::Fail, ranges.clone()).await,
        serve(Mode::Half, ranges.clone()).await,
        serve(Mode::Good, ranges.clone()).await,
    ];
    let e = entry(urls, sha_of_data());
    let dir = tmp("fallback");
    let r = fetch(e, &dir, &AtomicBool::new(false), &mut quiet()).await;
    assert!(r.is_ok(), "download failed");
    assert_eq!(std::fs::read(dir.join("model.gguf")).unwrap(), data());
    assert!(!part_of(&dir.join("model.gguf")).exists());
    // 第三个源是从第二个源断掉的地方接着下的
    assert_eq!(ranges.lock().unwrap().as_slice(), ["1500000-"]);
}

#[tokio::test]
async fn bad_checksum_deletes_the_file() {
    let ranges = Arc::new(Mutex::new(vec![]));
    let e = entry(vec![serve(Mode::Good, ranges).await], leak("0".repeat(64)));
    let dir = tmp("badsha");
    let r = fetch(e, &dir, &AtomicBool::new(false), &mut quiet()).await;
    assert!(matches!(r, Err(DlError::Failed(ref m)) if m.contains("校验失败")));
    assert!(!dir.join("model.gguf").exists());
    assert!(!part_of(&dir.join("model.gguf")).exists());
}

#[tokio::test]
async fn cancel_keeps_progress_and_resume_finishes() {
    let ranges = Arc::new(Mutex::new(vec![]));
    let half = serve(Mode::Half, ranges.clone()).await;
    let good = serve(Mode::Good, ranges.clone()).await;
    let dir = tmp("cancel");
    let part = part_of(&dir.join("model.gguf"));

    // 第一次：只有一个会断线的源，下到一半失败，.part 留着
    let only_half = entry(vec![half], sha_of_data());
    let r = fetch(only_half, &dir, &AtomicBool::new(false), &mut quiet()).await;
    assert!(matches!(r, Err(DlError::Failed(_))));
    assert_eq!(std::fs::metadata(&part).unwrap().len(), 1_500_000);

    // 第二次：刚开始核对已有部分就取消，进度不丢
    let e = entry(vec![good], sha_of_data());
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    let mut stop = move |state: &'static str, _: u64, _: u64, _: Option<&str>, _: Option<String>| {
        if state == "verifying" {
            flag.store(true, Ordering::SeqCst);
        }
    };
    let r = fetch(e, &dir, &cancel, &mut stop).await;
    assert!(matches!(r, Err(DlError::Canceled)));
    assert_eq!(std::fs::metadata(&part).unwrap().len(), 1_500_000);

    // 第三次：接着下完，校验通过
    let r = fetch(e, &dir, &AtomicBool::new(false), &mut quiet()).await;
    assert!(r.is_ok());
    assert_eq!(std::fs::read(dir.join("model.gguf")).unwrap(), data());
    assert_eq!(ranges.lock().unwrap().as_slice(), ["1500000-"]);
}
