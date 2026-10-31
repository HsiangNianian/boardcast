// 桌面外壳。目前负责：
// - 拉起打包进来的 llama-server（llm.rs），前端用 `llm_status` 拿地址
// - 下载和管理模型（models.rs）
// - 语音输入：麦克风 → 断句 → 识别（voice.rs）
// - 用户自己填的 OpenAI 兼容远程模型（remote.rs）
// - 导出文件时的另存为对话框（export.rs）
// - 会议的保存和读取（meetings.rs）
// - 自动更新（updater.rs）

mod export;
mod llm;
mod meetings;
mod models;
mod remote;
mod updater;
mod voice;

use tauri::{Manager, RunEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updater::Pending::default())
        .manage(llm::Llm::default())
        .manage(models::Downloads::default())
        .manage(voice::Voice::default())
        .setup(|app| {
            // 启用了远程模型就先不拉起内置模型（不占显存、不预热），远程不可用时前端再叫它起来
            if remote::enabled(app.handle()) {
                llm::stand_by(app.handle().clone());
            } else {
                llm::start(app.handle().clone());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            llm::llm_status,
            llm::llm_restart,
            llm::llm_ensure,
            llm::llm_stop,
            models::models_list,
            models::model_download,
            models::model_cancel,
            models::model_select,
            models::model_delete,
            voice::voice_status,
            voice::voice_devices,
            voice::voice_start,
            voice::voice_stop,
            remote::remote_get,
            remote::remote_save,
            remote::remote_test,
            remote::remote_chat,
            export::export_save,
            meetings::meetings_list,
            meetings::meeting_load,
            meetings::meeting_save,
            meetings::meeting_delete,
            meetings::meeting_asset_save,
            meetings::meeting_asset_load,
            updater::update_check,
            updater::update_install,
        ])
        .build(tauri::generate_context!())
        .expect("error while building boardcast")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                voice::voice_stop(app.clone());
                app.state::<llm::Llm>().shutdown();
            }
        });
}
