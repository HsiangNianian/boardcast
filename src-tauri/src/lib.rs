// 桌面外壳。目前负责：
// - 拉起打包进来的 llama-server（llm.rs），前端用 `llm_status` 拿地址
// - 下载和管理模型（models.rs）
// - 语音输入：麦克风 → 断句 → 识别（voice.rs）
// - 用户自己填的 OpenAI 兼容远程模型（remote.rs）
// - 导出文件时的另存为对话框（export.rs）
// - 会议的保存和读取（meetings.rs）
// - 自动更新（updater.rs）
// - 按 arXiv 编号下载论文（arxiv.rs）

mod apple_speech;
mod arxiv;
mod diagnostics;
mod export;
mod llm;
mod meetings;
mod models;
mod remote;
mod shell;
mod updater;
mod voice;

use tauri::{Manager, RunEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 单实例要最先注册：再启动一次时转给开着的那个
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if args.iter().any(|a| a == shell::TOGGLE_ARG) {
                shell::toggle(app);
            } else {
                shell::show_window(app);
            }
        }))
        .plugin(shell::shortcut_plugin())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(shell::Shell::default())
        .manage(updater::Pending::default())
        .manage(export::SavedExports::default())
        .manage(llm::Llm::default())
        .manage(models::Downloads::default())
        .manage(remote::Requests::default())
        .manage(voice::Voice::default())
        .manage(apple_speech::Jobs::default())
        .on_menu_event(|app, event| shell::menu_action(app, event.id().as_ref()))
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" && !shell::exit_approved(window.app_handle()) {
                    api.prevent_close();
                    shell::request_exit(window.app_handle(), shell::ExitReason::Close);
                }
            }
        })
        .setup(|app| {
            shell::init_menu(app.handle())?;
            if let Err(e) = shell::init_tray(app.handle()) {
                eprintln!("托盘启动失败：{e}");
            }
            voice::init(app.handle());
            #[cfg(target_os = "linux")]
            shell::watch_webview(app.handle())?;
            if std::env::args_os().any(|arg| arg == shell::TOGGLE_ARG) {
                shell::toggle(app.handle());
            }
            // 启用了远程模型就先不拉起内置模型（不占显存、不预热），远程不可用时前端再叫它起来
            if remote::enabled(app.handle()) {
                llm::stand_by(app.handle().clone());
            } else {
                llm::start(app.handle().clone());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            diagnostics::diagnostics_info,
            shell::shortcut_set,
            shell::session_wayland,
            shell::shell_set_locale,
            shell::shell_show_window,
            shell::shell_take_toggle,
            shell::shell_pending_exit,
            shell::shell_resolve_exit,
            llm::llm_status,
            llm::llm_chat,
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
            voice::preview::voice_input_check_start,
            voice::preview::voice_input_check_stop,
            voice::voice_models,
            voice::voice_select,
            voice::voice_prepare,
            remote::remote_get,
            remote::remote_save,
            remote::remote_test,
            remote::remote_prepare,
            remote::remote_cancel,
            remote::remote_chat,
            export::export_save,
            export::export_open,
            export::export_reveal,
            arxiv::arxiv_pdf,
            meetings::meetings_list,
            meetings::meeting_load,
            meetings::meeting_save,
            meetings::meeting_delete,
            meetings::meeting_asset_save,
            meetings::meeting_asset_load,
            updater::update_check,
            updater::update_download,
            updater::update_install,
        ])
        .build(tauri::generate_context!())
        .expect("error while building boardcast")
        .run(|app, event| {
            match event {
                // Tauri cannot prevent restart; updater only reaches it after onBeforeInstall saves.
                RunEvent::ExitRequested { code, api, .. }
                    if code != Some(tauri::RESTART_EXIT_CODE) && !shell::exit_approved(app) =>
                {
                    api.prevent_exit();
                    shell::request_exit(app, shell::ExitReason::Quit);
                }
                RunEvent::Exit => {
                    voice::shutdown(app);
                    app.state::<llm::Llm>().shutdown();
                }
                _ => {}
            }
        });
}
