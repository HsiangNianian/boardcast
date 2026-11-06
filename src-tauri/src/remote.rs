//! 远程模型：用户自己填的 OpenAI 兼容接口（base URL + API key + 模型名）。
//!
//! - 配置存在 `<应用配置目录>/remote.json`，Linux / macOS 上权限设成 0600；API key 不回传给前端，只给末尾几位。
//! - 请求由这里代发：走系统代理，不受 WebView 跨域和 CSP 的限制，key 也不进网页。
//! - 只发 OpenAI 标准字段；llama.cpp 专有的 grammar 等参数由前端按接口类型决定发不发。

use futures_util::future::{AbortHandle, AbortRegistration, Abortable};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const TEST_TIMEOUT: Duration = Duration::from_secs(30);
const CHAT_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Default)]
pub struct Requests {
    next: AtomicU64,
    active: Mutex<HashMap<String, (AbortHandle, Option<AbortRegistration>)>>,
}

impl Requests {
    fn prepare(&self) -> String {
        let id = self.next.fetch_add(1, Ordering::Relaxed).to_string();
        let (handle, registration) = AbortHandle::new_pair();
        self.active
            .lock()
            .unwrap()
            .insert(id.clone(), (handle, Some(registration)));
        id
    }

    fn cancel(&self, id: &str) {
        if let Some((handle, _)) = self.active.lock().unwrap().remove(id) {
            handle.abort();
        }
    }

    pub(crate) fn cancel_all(&self) {
        for (_, (handle, _)) in self.active.lock().unwrap().drain() {
            handle.abort();
        }
    }

    pub(crate) async fn run<T>(
        &self,
        id: &str,
        request: impl std::future::Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        let registration = self
            .active
            .lock()
            .unwrap()
            .get_mut(id)
            .and_then(|(_, registration)| registration.take())
            .ok_or("Request canceled or already started")?;
        let result = Abortable::new(request, registration)
            .await
            .unwrap_or_else(|_| Err("Request canceled".into()));
        self.cancel(id);
        result
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteConfig {
    base_url: String,
    api_key: String,
    model: String,
    enabled: bool,
}

/// 给前端看的样子：没有 key 本身
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteView {
    base_url: String,
    model: String,
    enabled: bool,
    /// 比如「sk-…a1b2」；没填 key 时为空
    key_hint: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    ms: u64,
    reply: String,
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    let d = app.path().app_config_dir().ok()?;
    let _ = std::fs::create_dir_all(&d);
    Some(d.join("remote.json"))
}

fn load(app: &AppHandle) -> RemoteConfig {
    path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn store(app: &AppHandle, c: &RemoteConfig) -> Result<(), String> {
    let p = path(app).ok_or("App configuration folder not found")?;
    let text = serde_json::to_string_pretty(c).map_err(|e| e.to_string())?;
    std::fs::write(&p, text).map_err(|e| format!("Save failed: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn view(c: &RemoteConfig) -> RemoteView {
    let key_hint = (!c.api_key.is_empty()).then(|| {
        if c.api_key.chars().count() <= 7 {
            return "••••".to_string();
        }
        let tail: String = c
            .api_key
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let head: String = c.api_key.chars().take(3).collect();
        format!("{head}…{tail}")
    });
    RemoteView {
        base_url: c.base_url.clone(),
        model: c.model.clone(),
        enabled: c.enabled,
        key_hint,
    }
}

/// 用户可能填 `https://api.openai.com/v1`，也可能直接填到 `/chat/completions`
pub fn chat_url(base: &str) -> String {
    let b = base.trim().trim_end_matches('/');
    if b.ends_with("/chat/completions") {
        b.to_string()
    } else {
        format!("{b}/chat/completions")
    }
}

fn is_loopback(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https"))
        .and_then(|u| u.host_str().map(str::to_string))
        .is_some_and(|h| {
            h == "localhost"
                || h.ends_with(".localhost")
                || h.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| {
                        ip.is_loopback()
                            || ip == std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)
                    })
        })
}

fn redact(text: &str, key: &str) -> String {
    if key.is_empty() {
        text.to_string()
    } else {
        text.replace(key, "[key]")
    }
}

fn network_error(error: reqwest::Error, key: &str) -> String {
    if error.is_timeout() {
        "Request timed out. Check the address and try again.".into()
    } else {
        let mut source = std::error::Error::source(&error);
        while let Some(cause) = source {
            // shortcut: these causes have no public typed API; recheck with reqwest/rustls upgrades.
            let detail = cause.to_string();
            if detail.contains("invalid peer certificate") {
                return "REMOTE_TLS_CERTIFICATE".into();
            }
            if detail.contains("proxy authorization required") {
                return "REMOTE_PROXY_AUTH".into();
            }
            if detail.starts_with("tunnel error:") {
                return "REMOTE_PROXY_CONNECT".into();
            }
            source = cause.source();
        }
        format!(
            "Can't connect: {}",
            redact(&error.without_url().to_string(), key)
        )
    }
}

pub(crate) async fn post(
    base: &str,
    key: &str,
    body: &Value,
    timeout: Duration,
) -> Result<Value, String> {
    if base.trim().is_empty() {
        return Err("Fill in the base URL first".into());
    }
    let invalid_url = "Enter an http:// or https:// address without a username or password.";
    let url = reqwest::Url::parse(&chat_url(base)).map_err(|_| invalid_url.to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(invalid_url.into());
    }
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout);
    // 远程服务一般要走系统代理；但填的是本机地址（LM Studio、本地 vLLM）时，走代理只会绕一圈失败
    if is_loopback(url.as_str()) {
        builder = builder.no_proxy();
    }
    let client = builder.build().map_err(|e| network_error(e, key))?;
    let mut req = client.post(url).json(body);
    if !key.is_empty() {
        req = req.bearer_auth(key);
    }
    let resp = req.send().await.map_err(|e| network_error(e, key))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| network_error(e, key))?;
    if !status.is_success() {
        // 服务商一般会在 error.message 里说清楚原因（key 不对、模型名不对……）
        let msg = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| {
                v.pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or(text);
        return Err(format!(
            "HTTP {status}: {}",
            redact(&msg, key).chars().take(300).collect::<String>()
        ));
    }
    let mut value: Value = serde_json::from_str(&text).map_err(|_| {
        "The service did not return JSON. Check that this is an OpenAI-compatible API address."
    })?;
    if !value.get("choices").is_some_and(Value::is_array)
        || !matches!(
            value.pointer("/choices/0/message/content"),
            Some(Value::String(_) | Value::Null)
        )
    {
        return Err("The reply has no valid choices[0].message.content — this doesn't look like an OpenAI-compatible API".into());
    }
    if let Some(Value::String(content)) = value.pointer_mut("/choices/0/message/content") {
        *content = redact(content, key);
    }
    Ok(value)
}

/// 用户启用了远程模型（不管现在连不连得上）
pub fn enabled(app: &AppHandle) -> bool {
    let c = load(app);
    c.enabled && !c.base_url.trim().is_empty() && !c.model.trim().is_empty()
}

#[tauri::command]
pub fn remote_get(app: AppHandle) -> RemoteView {
    view(&load(&app))
}

/// `api_key` 为 None 时保留原来的 key（表单里没改 key 的情况）
#[tauri::command]
pub fn remote_save(
    app: AppHandle,
    base_url: String,
    api_key: Option<String>,
    model: String,
    enabled: bool,
) -> Result<RemoteView, String> {
    let mut c = load(&app);
    c.base_url = base_url.trim().to_string();
    if let Some(k) = api_key {
        c.api_key = k.trim().to_string();
    }
    c.model = model.trim().to_string();
    c.enabled = enabled;
    store(&app, &c)?;
    Ok(view(&c))
}

/// 真的发一条最小的对话请求，确认地址、key、模型名都对。参数为 None 时用已保存的值
#[tauri::command]
pub async fn remote_test(
    app: AppHandle,
    base_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
) -> Result<TestResult, String> {
    let saved = load(&app);
    let base = base_url.unwrap_or(saved.base_url);
    let key = api_key.unwrap_or(saved.api_key);
    let model = model.unwrap_or(saved.model);
    if model.trim().is_empty() {
        return Err("Fill in the model name first".into());
    }
    let body = serde_json::json!({
        "model": model.trim(),
        "messages": [{ "role": "user", "content": "只回复 OK 两个字母。" }],
        "max_tokens": 8,
        "temperature": 0,
    });
    let t0 = Instant::now();
    let v = post(&base, key.trim(), &body, TEST_TIMEOUT).await?;
    let reply = v
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or("The reply has no choices[0].message.content — this doesn't look like an OpenAI-compatible API")?
        .trim()
        .chars()
        .take(40)
        .collect();
    Ok(TestResult {
        ms: t0.elapsed().as_millis() as u64,
        reply,
    })
}

// Acknowledging registration before a request prevents an early cancel from racing past it.
#[tauri::command]
pub fn remote_prepare(requests: tauri::State<'_, Requests>) -> String {
    requests.prepare()
}

#[tauri::command]
pub fn remote_cancel(requests: tauri::State<'_, Requests>, request_id: String) {
    requests.cancel(&request_id);
}

/// 用已保存的配置发一次对话请求；`body` 里的 model 会被换成配置里的模型名
#[tauri::command]
pub async fn remote_chat(
    app: AppHandle,
    requests: tauri::State<'_, Requests>,
    request_id: String,
    mut body: Value,
) -> Result<Value, String> {
    requests
        .run(&request_id, async {
            let c = load(&app);
            if !c.enabled {
                return Err("Remote model is not enabled".into());
            }
            body["model"] = Value::String(c.model.clone());
            post(&c.base_url, &c.api_key, &body, CHAT_TIMEOUT).await
        })
        .await
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[tokio::test]
    async fn lost_view_cancels_running_and_prepared_requests_without_reusing_ids() {
        let requests = Requests::default();
        let first = requests.prepare();
        let second = requests.prepare();
        let mut running = Box::pin(requests.run::<String>(&first, std::future::pending()));
        assert!(futures_util::poll!(running.as_mut()).is_pending());
        requests.cancel_all();
        assert_eq!(running.await.unwrap_err(), "Request canceled");
        assert!(requests
            .run::<String>(&second, async { panic!("stale request ran") })
            .await
            .is_err());
        assert!(requests.active.lock().unwrap().is_empty());
        let restored = requests.prepare();
        assert_ne!(restored, first);
        assert_ne!(restored, second);
        requests.cancel(&first);
        assert_eq!(
            requests
                .run(&restored, async { Ok("restored") })
                .await
                .unwrap(),
            "restored"
        );
    }

    #[tokio::test]
    async fn request_cancellation_is_isolated_and_cleans_up_every_outcome() {
        let requests = Requests::default();
        let canceled = requests.prepare();
        requests.cancel(&canceled);
        assert!(requests
            .run::<Value>(&canceled, async { panic!("canceled request was polled") })
            .await
            .is_err());
        assert!(requests
            .run::<Value>("unknown", async { panic!("unknown request was polled") })
            .await
            .is_err());

        let first_id = requests.prepare();
        let second_id = requests.prepare();
        assert_ne!(first_id, second_id);
        let mut first = Box::pin(requests.run::<Value>(&first_id, std::future::pending()));
        assert!(futures_util::poll!(first.as_mut()).is_pending());
        assert!(requests
            .run::<Value>(&first_id, async { panic!("duplicate request was polled") })
            .await
            .is_err());
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let mut second = Box::pin(requests.run(&second_id, async { receiver.await.unwrap() }));
        assert!(futures_util::poll!(second.as_mut()).is_pending());
        requests.cancel(&first_id);
        assert_eq!(first.await.unwrap_err(), "Request canceled");
        assert!(futures_util::poll!(second.as_mut()).is_pending());
        sender.send(Ok(serde_json::json!({"kept": true}))).unwrap();
        assert_eq!(second.await.unwrap(), serde_json::json!({"kept": true}));
        requests.cancel(&first_id);
        requests.cancel(&second_id);
        let failed = requests.prepare();
        assert_eq!(
            requests
                .run::<Value>(&failed, async { Err("original error".into()) })
                .await
                .unwrap_err(),
            "original error"
        );
        assert!(requests.active.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cancellation_closes_http_while_waiting_for_headers_or_body() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        for partial_body in [false, true] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}/v1", listener.local_addr().unwrap());
            let (ready, received) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let read = socket.read(&mut buffer).await.unwrap();
                    assert!(read > 0);
                    request.extend_from_slice(&buffer[..read]);
                    if String::from_utf8_lossy(&request).ends_with("{}") {
                        break;
                    }
                }
                if partial_body {
                    socket
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n{\"choices\":[")
                        .await
                        .unwrap();
                }
                ready.send(()).unwrap();
                let closed = tokio::time::timeout(Duration::from_secs(2), socket.read(&mut buffer))
                    .await
                    .expect("cancel must close the held HTTP connection");
                assert!(matches!(closed, Ok(0) | Err(_)), "{closed:?}");
            });
            let requests = Requests::default();
            let id = requests.prepare();
            let body = serde_json::json!({});
            let request = requests.run(&id, post(&base, "", &body, CHAT_TIMEOUT));
            let cancel = async {
                received.await.unwrap();
                requests.cancel(&id);
            };
            let (result, ()) = tokio::time::timeout(Duration::from_secs(3), async {
                tokio::join!(request, cancel)
            })
            .await
            .expect("cancel must resolve without waiting for the request timeout");
            assert_eq!(result.unwrap_err(), "Request canceled");
            server.await.unwrap();
            assert!(requests.active.lock().unwrap().is_empty());
        }
    }

    pub(crate) async fn reply_server(
        status: &str,
        body: &str,
        stall: bool,
    ) -> (String, tokio::task::JoinHandle<Vec<u8>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let response = if stall {
            "HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n{\"choices\":[".to_string()
        } else {
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        };
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let read = socket.read(&mut buffer).await.unwrap();
                assert!(read > 0);
                request.extend_from_slice(&buffer[..read]);
                if String::from_utf8_lossy(&request)
                    .split_once("\r\n\r\n")
                    .is_some_and(|(_, body)| serde_json::from_str::<Value>(body).is_ok())
                {
                    break;
                }
            }
            socket.write_all(response.as_bytes()).await.unwrap();
            if stall {
                let closed =
                    tokio::time::timeout(Duration::from_secs(35), socket.read(&mut buffer))
                        .await
                        .expect("client must close its timed-out response");
                assert!(matches!(closed, Ok(0) | Err(_)));
            } else {
                socket.shutdown().await.unwrap();
            }
            request
        });
        (format!("http://{address}/v1"), server)
    }

    #[tokio::test]
    async fn errors_and_successful_replies_hide_the_authentication_credential() {
        let key = "synthetic-nonstandard-credential-37fc8d";
        let cases = [
            ("401 Unauthorized", serde_json::json!({"error": {"message": format!("Invalid {key} {}", "x".repeat(500))}}).to_string(), "HTTP 401"),
            ("401 Unauthorized", format!("Invalid credential: {key}"), "HTTP 401"),
            ("200 OK", format!("Not JSON: {key}"), "The service did not return JSON"),
            ("200 OK", "{}".into(), "The reply has no valid choices"),
            ("200 OK", "null".into(), "The reply has no valid choices"),
            ("200 OK", serde_json::json!({"choices": [{"message": {"content": {"unexpected": key}}}]}).to_string(), "The reply has no valid choices"),
        ];
        for (status, body, expected) in cases {
            let (base, server) = reply_server(status, &body, false).await;
            let error = post(
                &base,
                key,
                &serde_json::json!({"probe": "OK"}),
                TEST_TIMEOUT,
            )
            .await
            .unwrap_err();
            assert!(error.starts_with(expected), "{error}");
            assert!(!error.contains(key));
            assert!(error.len() < 400);
            let request = server.await.unwrap();
            assert!(String::from_utf8_lossy(&request).contains(key));
        }
        let (base, server) = reply_server(
            "200 OK",
            &serde_json::json!({"choices": [{"message": {"content": key}}]}).to_string(),
            false,
        )
        .await;
        let reply = post(
            &base,
            key,
            &serde_json::json!({"probe": "OK"}),
            TEST_TIMEOUT,
        )
        .await
        .unwrap();
        assert_eq!(
            reply
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str),
            Some("[key]")
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_unsupported_or_credential_bearing_urls() {
        for url in [
            "not a URL",
            "ftp://localhost/v1",
            "http://user:password@127.0.0.1:9/v1",
        ] {
            assert!(post(url, "", &serde_json::json!({}), TEST_TIMEOUT)
                .await
                .unwrap_err()
                .starts_with("Enter an http:// or https://"));
        }
    }

    #[tokio::test]
    async fn proxy_failures_keep_recovery_reasons_without_exposing_credentials() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        for (status, expected) in [
            (
                Some("407 Proxy Authentication Required"),
                "REMOTE_PROXY_AUTH",
            ),
            (Some("502 Bad Gateway"), "REMOTE_PROXY_CONNECT"),
            (None, "REMOTE_PROXY_CONNECT"),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                while !request.ends_with(b"\r\n\r\n") {
                    let read = socket.read(&mut buffer).await.unwrap();
                    assert!(read > 0 && request.len() < 8192);
                    request.extend_from_slice(&buffer[..read]);
                }
                if let Some(status) = status {
                    socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
                }
                socket.shutdown().await.unwrap();
                String::from_utf8(request).unwrap()
            });
            let key = "synthetic-api-key";
            let error = reqwest::Client::builder()
                .proxy(
                    reqwest::Proxy::all(format!("http://{address}"))
                        .unwrap()
                        .basic_auth("fixture-user", "fixture-password"),
                )
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap()
                .post("https://boardcast.invalid/v1?secret=synthetic-query")
                .bearer_auth(key)
                .send()
                .await
                .unwrap_err();
            assert_eq!(network_error(error, key), expected);
            let request = server.await.unwrap();
            assert!(request.starts_with("CONNECT boardcast.invalid:443 "));
            assert!(!request.contains(key) && !request.contains("synthetic-query"));
        }
    }

    #[tokio::test]
    async fn connection_test_timeout_includes_a_stalled_response_body() {
        let (base, server) = reply_server("", "", true).await;
        let start = Instant::now();
        let error = post(&base, "", &serde_json::json!({"probe": "OK"}), TEST_TIMEOUT)
            .await
            .unwrap_err();
        assert!(error.contains("timed out"), "{error}");
        assert!(
            start.elapsed() >= Duration::from_secs(29) && start.elapsed() < Duration::from_secs(40)
        );
        server.await.unwrap();
    }

    #[test]
    fn chat_url_accepts_both_forms() {
        assert_eq!(
            chat_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            chat_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            chat_url(" https://x.ai/v1/chat/completions "),
            "https://x.ai/v1/chat/completions"
        );
    }

    /// 对着一个真的 OpenAI 兼容服务发请求（比如本地 llama-server），默认跳过：
    ///   BOARDCAST_REMOTE_URL=http://127.0.0.1:8080/v1 cargo test --lib remote -- --ignored
    #[tokio::test]
    #[ignore]
    async fn talks_to_a_real_openai_compatible_server() {
        let base = std::env::var("BOARDCAST_REMOTE_URL").expect("设置 BOARDCAST_REMOTE_URL");
        let body = serde_json::json!({
            "model": "any",
            "messages": [{ "role": "user", "content": "只回复 OK 两个字母。" }],
            "max_tokens": 8,
        });
        let v = post(&base, "", &body, TEST_TIMEOUT)
            .await
            .expect("请求失败");
        assert!(v.pointer("/choices/0/message/content").is_some(), "{v}");
        // 地址写错：要给出看得懂的 HTTP 错误，而不是解析失败
        let err = post(&format!("{base}/nope"), "", &body, TEST_TIMEOUT)
            .await
            .unwrap_err();
        assert!(err.starts_with("HTTP 404"), "{err}");
        // 连不上的地址
        let err = post("http://127.0.0.1:9/v1", "", &body, TEST_TIMEOUT)
            .await
            .unwrap_err();
        assert!(err.starts_with("Can't connect"), "{err}");
    }

    #[test]
    fn loopback_addresses_skip_the_proxy() {
        assert!(is_loopback("http://127.0.0.1:1234/v1/chat/completions"));
        assert!(is_loopback("http://localhost:11434/v1/chat/completions"));
        assert!(is_loopback("http://[::1]:8000/v1/chat/completions"));
        assert!(is_loopback("http://127.1:8000/v1"));
        assert!(is_loopback("http://lm.localhost:8000/v1"));
        assert!(!is_loopback("http://127.boardcast.test/v1"));
        assert!(!is_loopback("http://127.0.0.1.example/v1"));
        assert!(!is_loopback("ftp://localhost/v1"));
        assert!(!is_loopback("https://api.openai.com/v1/chat/completions"));
    }

    #[tokio::test]
    async fn refuses_redirect_before_forwarding_meeting_content() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut received = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let read = socket.read(&mut buffer).await.unwrap();
                assert!(read > 0);
                received.extend_from_slice(&buffer[..read]);
                if String::from_utf8_lossy(&received).ends_with("}") {
                    break;
                }
            }
            let response = format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{address}/unapproved\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.shutdown().await.unwrap();
            (listener, received)
        });
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            post(
                &format!("http://{address}/v1"),
                "synthetic-test-key",
                &serde_json::json!({"messages": "PRIVATE-REDIRECT-CONTENT"}),
                TEST_TIMEOUT,
            ),
        )
        .await
        .expect("redirect must return without a second request");
        assert!(result.unwrap_err().starts_with("HTTP 307"));
        let (listener, request) = server.await.unwrap();
        assert!(String::from_utf8_lossy(&request).contains("PRIVATE-REDIRECT-CONTENT"));
        assert!(
            tokio::time::timeout(Duration::from_millis(30), listener.accept())
                .await
                .is_err()
        );
    }

    #[test]
    fn key_hint_never_contains_the_middle_of_the_key() {
        let c = RemoteConfig {
            api_key: "sk-1234567890abcdef".into(),
            ..Default::default()
        };
        let hint = view(&c).key_hint.unwrap();
        assert_eq!(hint, "sk-…cdef");
        assert!(!hint.contains("123456"));
        assert_eq!(
            view(&RemoteConfig {
                api_key: "abcdef".into(),
                ..Default::default()
            })
            .key_hint
            .as_deref(),
            Some("••••")
        );
    }
}
