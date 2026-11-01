//! 导出：弹系统的「另存为」对话框，把前端生成好的文件写下去。
//! WebView 里的 <a download> 在 Linux（WebKitGTK）上不一定管用，所以桌面版走这里。

use base64::Engine as _;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

/// 用户取消时返回 None，保存成功返回写到的路径
#[tauri::command]
pub async fn export_save(
    app: AppHandle,
    name: String,
    data_base64: String,
) -> Result<Option<String>, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64.as_bytes())
        .map_err(|e| format!("Invalid file data: {e}"))?;
    let ext = std::path::Path::new(&name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_string();
    let label = match ext.as_str() {
        "png" => "PNG image",
        "svg" => "SVG vector",
        "md" => "Markdown",
        "excalidraw" => "Excalidraw",
        "html" => "Replay page",
        "pdf" => "PDF",
        "json" => "JSON",
        "jsonl" => "JSON Lines",
        _ => "File",
    };
    // 对话框会阻塞，放到专门的线程里等
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let mut d = app.dialog().file().set_file_name(&name);
        if !ext.is_empty() {
            d = d.add_filter(label, &[ext.as_str()]);
        }
        d.blocking_save_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(path) = picked else { return Ok(None) };
    let path = path.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| format!("Save failed: {e}"))?;
    Ok(Some(path.display().to_string()))
}
