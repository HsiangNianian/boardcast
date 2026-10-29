//! 远程模型：用户自己填的 OpenAI 兼容接口（base URL + API key + 模型名）。
//!
//! - 配置存在 `<应用配置目录>/remote.json`，Linux / macOS 上权限设成 0600；API key 不回传给前端，只给末尾几位。
//! - 请求由这里代发：走系统代理，不受 WebView 跨域和 CSP 的限制，key 也不进网页。
//! - 只发 OpenAI 标准字段；llama.cpp 专有的 grammar 等参数由前端按接口类型决定发不发。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

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
    let p = path(app).ok_or("找不到应用配置目录")?;
    let text = serde_json::to_string_pretty(c).map_err(|e| e.to_string())?;
    std::fs::write(&p, text).map_err(|e| format!("保存失败：{e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn view(c: &RemoteConfig) -> RemoteView {
    let key_hint = (!c.api_key.is_empty()).then(|| {
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
        .and_then(|u| u.host_str().map(str::to_string))
        .is_some_and(|h| h == "localhost" || h.starts_with("127.") || h == "[::1]" || h == "::1")
}

async fn post(base: &str, key: &str, body: &Value) -> Result<Value, String> {
    if base.trim().is_empty() {
        return Err("还没填 base URL".into());
    }
    let url = chat_url(base);
    let mut builder = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60));
    // 远程服务一般要走系统代理；但填的是本机地址（LM Studio、本地 vLLM）时，走代理只会绕一圈失败
    if is_loopback(&url) {
        builder = builder.no_proxy();
    }
    let client = builder.build().map_err(|e| e.to_string())?;
    let mut req = client.post(&url).json(body);
    if !key.is_empty() {
        req = req.bearer_auth(key);
    }
    let resp = req.send().await.map_err(|e| format!("连不上：{e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        // 服务商一般会在 error.message 里说清楚原因（key 不对、模型名不对……）
        let msg = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| {
                v.pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| text.chars().take(300).collect());
        return Err(format!("HTTP {status}：{msg}"));
    }
    serde_json::from_str(&text).map_err(|_| {
        format!(
            "返回的不是 JSON：{}",
            text.chars().take(200).collect::<String>()
        )
    })
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
        return Err("还没填模型名".into());
    }
    let body = serde_json::json!({
        "model": model.trim(),
        "messages": [{ "role": "user", "content": "只回复 OK 两个字母。" }],
        "max_tokens": 8,
        "temperature": 0,
    });
    let t0 = Instant::now();
    let v = post(&base, &key, &body).await?;
    let reply = v
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or("返回里没有 choices[0].message.content，不像是 OpenAI 兼容接口")?
        .trim()
        .chars()
        .take(40)
        .collect();
    Ok(TestResult {
        ms: t0.elapsed().as_millis() as u64,
        reply,
    })
}

/// 用已保存的配置发一次对话请求；`body` 里的 model 会被换成配置里的模型名
#[tauri::command]
pub async fn remote_chat(app: AppHandle, mut body: Value) -> Result<Value, String> {
    let c = load(&app);
    if !c.enabled {
        return Err("远程模型没有启用".into());
    }
    body["model"] = Value::String(c.model.clone());
    post(&c.base_url, &c.api_key, &body).await
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let v = post(&base, "", &body).await.expect("请求失败");
        assert!(v.pointer("/choices/0/message/content").is_some(), "{v}");
        // 地址写错：要给出看得懂的 HTTP 错误，而不是解析失败
        let err = post(&format!("{base}/nope"), "", &body).await.unwrap_err();
        assert!(err.starts_with("HTTP 404"), "{err}");
        // 连不上的地址
        let err = post("http://127.0.0.1:9/v1", "", &body).await.unwrap_err();
        assert!(err.starts_with("连不上"), "{err}");
    }

    #[test]
    fn loopback_addresses_skip_the_proxy() {
        assert!(is_loopback("http://127.0.0.1:1234/v1/chat/completions"));
        assert!(is_loopback("http://localhost:11434/v1/chat/completions"));
        assert!(is_loopback("http://[::1]:8000/v1/chat/completions"));
        assert!(!is_loopback("https://api.openai.com/v1/chat/completions"));
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
    }
}
