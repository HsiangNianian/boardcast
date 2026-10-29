// 桌面外壳。目前负责：
// - 拉起打包进来的 llama-server（llm.rs），前端用 `llm_status` 拿地址
// - 下载和管理模型（models.rs）
// 之后会接：麦克风和系统声音采集、sherpa-onnx 流式转写

mod llm;
mod models;

use tauri::{Manager, RunEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(llm::Llm::default())
        .manage(models::Downloads::default())
        .setup(|app| {
            llm::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            llm::llm_status,
            llm::llm_restart,
            models::models_list,
            models::model_download,
            models::model_cancel,
            models::model_select,
            models::model_delete,
        ])
        .build(tauri::generate_context!())
        .expect("error while building boardcast")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                app.state::<llm::Llm>().shutdown();
            }
        });
}
