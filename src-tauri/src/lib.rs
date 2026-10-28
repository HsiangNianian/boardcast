// 目前外壳只负责开窗口。下一步会在这里接：
// - 麦克风和系统声音采集（cpal / PipeWire monitor / WASAPI loopback）
// - sherpa-onnx 流式转写
// - 把 llama-server 作为附带进程拉起来，并在首次启动时下载模型

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running boardcast");
}
