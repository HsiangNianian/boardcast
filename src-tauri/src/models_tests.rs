//! 用本地的假 HTTP 服务测下载逻辑，不依赖外网：
//! 源报错换下一个、下到一半断线接着下、续传带 Range、校验失败删文件、取消后保留进度。

use super::*;
use std::sync::Arc;
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
    (0..3_000_000u32)
        .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
        .collect()
}

/// 起一个假服务，返回地址；`ranges` 记下收到过的 Range 请求头
async fn serve(mode: Mode, ranges: Arc<Mutex<Vec<String>>>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        let body = data();
        loop {
            let Ok((mut s, _)) = l.accept().await else {
                return;
            };
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
                        let _ = s
                            .write_all(b"HTTP/1.1 500 Oops\r\ncontent-length: 0\r\n\r\n")
                            .await;
                    }
                    Mode::Half => {
                        let head =
                            format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n", body.len());
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

fn entry(urls: Vec<String>, sha256: &'static str) -> &'static FileSpec {
    let sources: Vec<Source> = urls
        .into_iter()
        .enumerate()
        .map(|(i, u)| Source {
            name: leak(format!("源{i}")),
            url: leak(u),
            direct: true,
        })
        .collect();
    Box::leak(Box::new(FileSpec {
        name: "model.gguf",
        size: 3_000_000,
        sha256,
        sources: Box::leak(sources.into_boxed_slice()),
    }))
}

#[test]
fn refuses_to_start_without_enough_space() {
    let dir = tmp("space");
    let e = CATALOG.iter().find(|e| e.id == "qwen3.5-4b").unwrap();
    // 2.7 GB 的模型，盘上只剩 1 GB：拒绝，并且说清楚差多少
    let err = check_space(e, &dir, 1_000_000_000).unwrap_err();
    assert!(
        err.contains("Not enough disk space") && err.contains("2.7 GB"),
        "{err}"
    );
    // 已经下了 2 GB：只需要再下 0.7 GB，加余量 1 GB 够了
    std::fs::write(part_of(&dir.join(e.main_file())), vec![0u8; 0]).unwrap();
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(part_of(&dir.join(e.main_file())))
        .unwrap();
    f.set_len(2_000_000_000).unwrap();
    assert!(check_space(e, &dir, 1_100_000_000).is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn voice_entry_points_at_the_files_the_engine_loads() {
    let e = CATALOG.iter().find(|e| e.id == "voice").unwrap();
    let names: Vec<_> = e.files.iter().map(|f| f.name).collect();
    for want in [
        voice_files::SENSE_VOICE,
        voice_files::TOKENS,
        voice_files::VAD,
    ] {
        assert!(names.contains(&want), "{want}");
    }
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

#[test]
fn model_choice_follows_selection_and_remaining_files() {
    assert!(
        std::env::var_os("BOARDCAST_MODEL").is_none(),
        "Unset BOARDCAST_MODEL for this fixture"
    );
    let dir = tmp("selection");
    let four = dir.join("Qwen3.5-4B-Q4_K_M.gguf");
    let nine = dir.join("Qwen3.5-9B-Q4_K_M.gguf");
    let vision = dir.join("Qwen3.5-4B-mmproj-F16.gguf");
    for path in [&four, &nine, &vision] {
        std::fs::write(path, b"fixture").unwrap();
    }
    assert_eq!(llm::pick_model(&dir), Some(nine.clone()));
    std::fs::write(dir.join(llm::SELECTED), "Qwen3.5-4B-Q4_K_M.gguf\n").unwrap();
    assert_eq!(llm::pick_model(&dir), Some(four.clone()));
    std::fs::write(dir.join(llm::SELECTED), "Qwen3.5-9B-Q4_K_M.gguf").unwrap();
    assert_eq!(llm::pick_model(&dir), Some(nine.clone()));
    std::fs::remove_file(nine).unwrap();
    assert_eq!(llm::pick_model(&dir), Some(four.clone()));
    std::fs::remove_file(dir.join(llm::SELECTED)).unwrap();
    assert_eq!(llm::pick_model(&dir), Some(four.clone()));
    std::fs::remove_file(four).unwrap();
    assert_eq!(llm::pick_model(&dir), None);
    assert!(vision.is_file());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn each_model_gets_its_own_vision_component() {
    let d = tmp("vision");
    // 没装看图组件
    assert_eq!(vision_for(&d, "Qwen3.5-4B-Q4_K_M.gguf"), None);
    std::fs::write(d.join("Qwen3.5-4B-mmproj-F16.gguf"), b"x").unwrap();
    assert_eq!(
        vision_for(&d, "Qwen3.5-4B-Q4_K_M.gguf"),
        Some(d.join("Qwen3.5-4B-mmproj-F16.gguf"))
    );
    // 4B 的组件不能给 9B 用
    assert_eq!(vision_for(&d, "Qwen3.5-9B-Q4_K_M.gguf"), None);
    // 每个大模型都有配对的组件
    for e in CATALOG.iter().filter(|e| e.kind == Kind::Llm) {
        let id = format!("{}-vision", e.id);
        assert!(
            CATALOG.iter().any(|v| v.id == id && v.kind == Kind::Vision),
            "{id}"
        );
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// 真的从清单里的源下载轻量档模型（2.7 GB），默认跳过：
///   cargo test --manifest-path src-tauri/Cargo.toml --lib -- --ignored --nocapture
#[tokio::test]
#[ignore]
async fn real_download_of_the_small_model() {
    let e = &CATALOG[0].files[0];
    let dir = tmp("real");
    let start = std::time::Instant::now();
    let mut last = 0u64;
    let (_cancel, mut canceled) = watch::channel(false);
    let r = fetch(
        e,
        &dir,
        &mut canceled,
        &mut |state, have, speed, source, _| {
            if have >= last + 500_000_000 || state != "downloading" {
                last = have;
                eprintln!(
                    "{state} {:.2} GB {:.1} MB/s {}",
                    have as f64 / 1e9,
                    speed as f64 / 1e6,
                    source.unwrap_or("")
                );
            }
        },
    )
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
    let (_cancel, mut canceled) = watch::channel(false);
    let r = fetch(e, &dir, &mut canceled, &mut quiet()).await;
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
    let (_cancel, mut canceled) = watch::channel(false);
    let r = fetch(e, &dir, &mut canceled, &mut quiet()).await;
    assert!(matches!(r, Err(DlError::Failed(ref m)) if m.contains("Verification failed")));
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
    let (cancel, mut canceled) = watch::channel(false);
    let r = fetch(only_half, &dir, &mut canceled, &mut quiet()).await;
    assert!(matches!(r, Err(DlError::Failed(_))));
    assert_eq!(std::fs::metadata(&part).unwrap().len(), 1_500_000);

    // 第二次：刚开始核对已有部分就取消，进度不丢
    let e = entry(vec![good], sha_of_data());
    let flag = cancel.clone();
    let mut stop =
        move |state: &'static str, _: u64, _: u64, _: Option<&str>, _: Option<String>| {
            if state == "verifying" {
                flag.send_replace(true);
            }
        };
    let r = fetch(e, &dir, &mut canceled, &mut stop).await;
    assert!(matches!(r, Err(DlError::Canceled)));
    assert_eq!(std::fs::metadata(&part).unwrap().len(), 1_500_000);

    // 第三次：接着下完，校验通过
    let (_cancel, mut canceled) = watch::channel(false);
    let mut phases = Vec::new();
    let r = fetch(e, &dir, &mut canceled, &mut |state, have, _, source, _| {
        phases.push((state, have, source.map(str::to_string)));
    })
    .await;
    assert!(r.is_ok());
    assert_eq!(phases[0].0, "verifying");
    assert_eq!(phases[1], ("downloading", 1_500_000, Some("源0".into())));
    assert_eq!(std::fs::read(dir.join("model.gguf")).unwrap(), data());
    assert_eq!(ranges.lock().unwrap().as_slice(), ["1500000-"]);
}

#[tokio::test]
async fn cancel_interrupts_stalled_headers_and_body_without_trying_another_source() {
    for partial_body in [false, true] {
        let dir = tmp(if partial_body {
            "stalled-body"
        } else {
            "stalled-headers"
        });
        let part = part_of(&dir.join("model.gguf"));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fallback = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let e = entry(
            vec![
                format!("http://{}/model.gguf", listener.local_addr().unwrap()),
                format!("http://{}/model.gguf", fallback.local_addr().unwrap()),
            ],
            sha_of_data(),
        );
        let (ready, received) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            while !request.ends_with(b"\r\n\r\n") {
                let read = socket.read(&mut buffer).await.unwrap();
                assert!(read > 0);
                request.extend_from_slice(&buffer[..read]);
            }
            if partial_body {
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3000000\r\n\r\n")
                    .await
                    .unwrap();
                socket.write_all(&data()[..65536]).await.unwrap();
            }
            ready.send(()).unwrap();
            let closed = tokio::time::timeout(Duration::from_secs(3), socket.read(&mut buffer))
                .await
                .expect("pause must close the held connection");
            assert!(matches!(closed, Ok(0) | Err(_)), "{closed:?}");
        });
        let (cancel, mut canceled) = watch::channel(false);
        let mut report = quiet();
        let request = fetch(e, &dir, &mut canceled, &mut report);
        let pause = async {
            received.await.unwrap();
            if partial_body {
                tokio::time::timeout(Duration::from_secs(1), async {
                    while file_size(&part) != 65536 {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await
                .expect("partial data should reach disk before pause");
            }
            cancel.send_replace(true);
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(request, pause)
        })
        .await
        .expect("pause must not wait for the network read timeout");
        assert!(matches!(result, Err(DlError::Canceled)));
        server.await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(20), fallback.accept())
                .await
                .is_err()
        );
        assert_eq!(file_size(&part), if partial_body { 65536 } else { 0 });
        if partial_body {
            assert_eq!(std::fs::read(&part).unwrap(), data()[..65536]);
        }
        assert!(!dir.join("model.gguf").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[tokio::test]
async fn pre_canceled_download_does_not_create_a_partial_file_or_connect() {
    let parent = tmp("pre-canceled");
    let dir = parent.join("not-created");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let e = entry(
        vec![format!(
            "http://{}/model.gguf",
            listener.local_addr().unwrap()
        )],
        sha_of_data(),
    );
    let (_cancel, mut canceled) = watch::channel(true);
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        fetch(e, &dir, &mut canceled, &mut quiet()),
    )
    .await
    .unwrap();
    assert!(matches!(result, Err(DlError::Canceled)));
    assert!(!dir.exists());
    assert!(
        tokio::time::timeout(Duration::from_millis(20), listener.accept())
            .await
            .is_err()
    );
    let _ = std::fs::remove_dir_all(parent);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn buffered_write_failure_does_not_install_a_model() {
    let dir = tmp("flush-failure");
    let part = part_of(&dir.join("model.gguf"));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let e = entry(
        vec![format!(
            "http://{}/model.gguf",
            listener.local_addr().unwrap()
        )],
        leak(format!("{:x}", Sha256::digest(b"GGUF"))),
    );
    let tiny = FileSpec {
        name: e.name,
        size: 4,
        sha256: e.sha256,
        sources: e.sources,
    };
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        assert!(socket.read(&mut request).await.unwrap() > 0);
        // Create after resume inspection so only the write path reaches /dev/full.
        std::os::unix::fs::symlink("/dev/full", part).unwrap();
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nGGUF")
            .await
            .unwrap();
        socket.shutdown().await.unwrap();
    });
    let (_cancel, mut canceled) = watch::channel(false);
    let result = fetch(&tiny, &dir, &mut canceled, &mut quiet()).await;
    assert!(
        matches!(result, Err(DlError::Failed(ref message)) if message.contains("Write failed"))
    );
    assert!(!dir.join("model.gguf").exists());
    server.await.unwrap();
    let _ = std::fs::remove_dir_all(dir);
}
