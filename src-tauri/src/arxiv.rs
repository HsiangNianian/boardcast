//! 按 arXiv 编号下载论文 PDF（#32）。网页受 CSP 限制连不了外网、arXiv 也不允许浏览器跨域取，所以由外壳代下。
//! 只认 arXiv 的编号，只从 arxiv.org 下，限制大小，免得被当成任意下载器用。

use base64::Engine;
use std::time::Duration;

/// 最大 60 MB：论文 PDF 一般几 MB，带大量图片的也很少超过几十 MB
const MAX: usize = 60 * 1024 * 1024;

/// arXiv 编号：新式 `2401.12345`（可带 `v2`），旧式 `cs/0112017`、`math.GT/0309136`
pub fn valid_id(id: &str) -> bool {
    // 去掉版本号后缀（v2）
    let base = match id.rsplit_once('v') {
        Some((b, v)) if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()) => b,
        _ => id,
    };
    let new_style = base.split_once('.').is_some_and(|(a, b)| {
        a.len() == 4
            && a.chars().all(|c| c.is_ascii_digit())
            && (4..=5).contains(&b.len())
            && b.chars().all(|c| c.is_ascii_digit())
    });
    let old_style = base.split_once('/').is_some_and(|(arch, num)| {
        !arch.is_empty()
            && arch
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '-' || c == '.' || c.is_ascii_uppercase())
            && num.len() == 7
            && num.chars().all(|c| c.is_ascii_digit())
    });
    new_style || old_style
}

/// 下载 PDF，返回 base64
#[tauri::command]
pub async fn arxiv_pdf(id: String) -> Result<String, String> {
    let id = id.trim().to_string();
    if !valid_id(&id) {
        return Err(format!("Invalid arXiv ID: {id}"));
    }
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .user_agent("boardcast (https://github.com/HsiangNianian/boardcast)")
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("https://arxiv.org/pdf/{id}");
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Can't connect to arXiv: {e}"))?;
    // 跳转以后也必须还在 arxiv.org
    let host = resp.url().host_str().unwrap_or_default().to_string();
    if !(host == "arxiv.org" || host.ends_with(".arxiv.org")) {
        return Err(format!("arXiv redirected to another site ({host}); download canceled"));
    }
    if !resp.status().is_success() {
        return Err(format!("arXiv returned {}: the ID may be incorrect", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| format!("Download failed: {e}"))?;
    if bytes.len() > MAX {
        return Err("PDF is too large (over 60 MB)".into());
    }
    if !bytes.starts_with(b"%PDF") {
        return Err("arXiv returned no PDF (the paper may only have source files or may still be processing)".into());
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真的从 arXiv 下一篇。要联网，默认跳过：
    ///   cargo test --manifest-path src-tauri/Cargo.toml --lib real_arxiv -- --ignored
    #[tokio::test]
    #[ignore]
    async fn real_arxiv_download() {
        let b64 = arxiv_pdf("1512.03385v1".into()).await.expect("下载失败");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        assert!(bytes.starts_with(b"%PDF") && bytes.len() > 100_000);
        assert!(arxiv_pdf("../x".into()).await.is_err());
    }

    #[test]
    fn only_arxiv_ids_are_accepted() {
        for ok in [
            "1512.03385",
            "1706.03762v7",
            "2401.12345",
            "cs/0112017",
            "math.GT/0309136v2",
        ] {
            assert!(valid_id(ok), "{ok}");
        }
        for bad in [
            "",
            "../etc/passwd",
            "1512.033",
            "https://evil.com/x.pdf",
            "1512.03385/../../x",
            "abc",
            "cs/01120",
        ] {
            assert!(!valid_id(bad), "{bad}");
        }
    }
}
