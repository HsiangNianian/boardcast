//! 原生菜单、托盘、全局快捷键与退出保存握手。
//! 收音仍由前端决定；关闭/退出必须等当前请求的保存确认，失败保留窗口。
//! Wayland 不让应用监听全局按键，那边把 `boardcast --toggle-mic` 绑到系统快捷键上。

use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use tauri::image::Image;
use tauri::menu::{
    AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID, WINDOW_SUBMENU_ID,
};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExitReason {
    Close,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ExitRequest {
    pub id: u64,
    pub reason: ExitReason,
}

#[derive(Default)]
struct ExitGate {
    next: u64,
    pending: Option<ExitRequest>,
    approved: bool,
}

impl ExitGate {
    fn restore_view(&mut self) {
        self.pending = None;
        self.approved = false;
    }

    fn request(&mut self, reason: ExitReason) -> Option<ExitRequest> {
        if self.pending.is_some() || self.approved {
            return None;
        }
        self.next += 1;
        let request = ExitRequest {
            id: self.next,
            reason,
        };
        self.pending = Some(request);
        Some(request)
    }

    fn resolve(&mut self, id: u64, allow: bool) -> Option<ExitRequest> {
        if self.pending.as_ref().is_none_or(|request| request.id != id) {
            return None;
        }
        self.approved = allow;
        self.pending.take()
    }
}

#[derive(Clone, Copy, Default)]
enum Locale {
    #[default]
    En,
    Zh,
}

impl Locale {
    fn text(self, en: &'static str, zh: &'static str) -> &'static str {
        if matches!(self, Self::Zh) {
            zh
        } else {
            en
        }
    }
}

#[derive(Default)]
pub struct Shell {
    exit: Mutex<ExitGate>,
    locale: Mutex<Locale>,
    listening: AtomicBool,
    shortcut: Mutex<Option<Shortcut>>,
    pending_toggle: AtomicBool,
}

impl Shell {
    fn request_exit(&self, reason: ExitReason) -> Option<ExitRequest> {
        let mut exit = self.exit.lock().unwrap();
        let request = exit.request(reason);
        if request.is_some() {
            self.pending_toggle.store(false, Ordering::Release);
        }
        request
    }

    fn queue_toggle(&self) {
        // Two requests before the frontend is ready cancel each other.
        let exit = self.exit.lock().unwrap();
        if exit.pending.is_none() && !exit.approved {
            self.pending_toggle.fetch_xor(true, Ordering::AcqRel);
        }
    }

    fn take_toggle(&self) -> bool {
        let exit = self.exit.lock().unwrap();
        let pending = self.pending_toggle.swap(false, Ordering::AcqRel);
        pending && exit.pending.is_none() && !exit.approved
    }
}

pub fn request_exit(app: &AppHandle, reason: ExitReason) {
    let request = app.state::<Shell>().request_exit(reason);
    if let Some(request) = request {
        show_window(app);
        // If the frontend is still loading, shell_pending_exit delivers this same token later.
        let _ = app.emit("shell-exit-request", request);
    }
}

pub fn exit_approved(app: &AppHandle) -> bool {
    app.state::<Shell>().exit.lock().unwrap().approved
}

#[tauri::command]
pub fn shell_pending_exit(app: AppHandle) -> Option<ExitRequest> {
    app.state::<Shell>().exit.lock().unwrap().pending
}

#[tauri::command]
pub fn shell_resolve_exit(app: AppHandle, id: u64, allow: bool) -> Result<bool, String> {
    let request = app.state::<Shell>().exit.lock().unwrap().resolve(id, allow);
    let Some(request) = request else {
        return Ok(false);
    };
    if !allow {
        return Ok(true);
    }
    match request.reason {
        ExitReason::Quit => app.exit(0),
        ExitReason::Close => {
            if let Some(window) = app.get_webview_window("main") {
                if let Err(error) = window.destroy() {
                    app.state::<Shell>().exit.lock().unwrap().approved = false;
                    return Err(error.to_string());
                }
            } else {
                app.exit(0);
            }
        }
    }
    Ok(true)
}

fn application_menu(app: &AppHandle, locale: Locale) -> tauri::Result<Menu<tauri::Wry>> {
    let text = |en, zh| locale.text(en, zh);
    let item = |id, en, zh, accelerator: Option<&str>| {
        MenuItem::with_id(app, id, text(en, zh), true, accelerator)
    };
    let about = AboutMetadata {
        name: Some(app.package_info().name.clone()),
        version: Some(app.package_info().version.to_string()),
        ..Default::default()
    };
    let application = Submenu::with_items(
        app,
        "boardcast",
        true,
        &[
            &PredefinedMenuItem::about(
                app,
                Some(text("About boardcast", "关于 boardcast")),
                Some(about),
            )?,
            &item("settings", "Settings…", "设置…", Some("CmdOrCtrl+,"))?,
            &item("check-updates", "Check for Updates…", "检查更新…", None)?,
            &PredefinedMenuItem::separator(app)?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::services(app, Some(text("Services", "服务")))?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::hide(app, Some(text("Hide boardcast", "隐藏 boardcast")))?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::hide_others(app, Some(text("Hide Others", "隐藏其他")))?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::show_all(app, Some(text("Show All", "全部显示")))?,
            &item(
                "app-quit",
                "Quit boardcast",
                "退出 boardcast",
                Some("CmdOrCtrl+Q"),
            )?,
        ],
    )?;
    let file = Submenu::with_items(
        app,
        text("File", "文件"),
        true,
        &[
            &item(
                "new-meeting",
                "New Meeting…",
                "新建会议…",
                Some("CmdOrCtrl+N"),
            )?,
            &item("meetings", "Meeting Library…", "会议库…", None)?,
            &PredefinedMenuItem::separator(app)?,
            &item(
                "import",
                "Import Material…",
                "导入材料…",
                Some("CmdOrCtrl+O"),
            )?,
            &item("export", "Export…", "导出…", None)?,
            &PredefinedMenuItem::separator(app)?,
            &item("app-close", "Close Window", "关闭窗口", Some("CmdOrCtrl+W"))?,
        ],
    )?;
    // Canvas commands use the app bridge; clipboard actions stay with the native focused responder.
    let edit = Submenu::with_items(
        app,
        text("Edit", "编辑"),
        true,
        &[
            &item("undo", "Undo", "撤销", Some("CmdOrCtrl+Z"))?,
            &item("redo", "Redo", "重做", Some("CmdOrCtrl+Shift+Z"))?,
            &PredefinedMenuItem::separator(app)?,
            // GTK's predefined actions inject X11 keys; use WebKit's focused editor on Linux.
            #[cfg(target_os = "linux")]
            &item("Cut", "Cut", "剪切", Some("CmdOrCtrl+X"))?,
            #[cfg(target_os = "linux")]
            &item("Copy", "Copy", "复制", Some("CmdOrCtrl+C"))?,
            #[cfg(target_os = "linux")]
            &item("Paste", "Paste", "粘贴", Some("CmdOrCtrl+V"))?,
            #[cfg(target_os = "linux")]
            &item("select-all", "Select All", "全选", Some("CmdOrCtrl+A"))?,
            #[cfg(not(target_os = "linux"))]
            &PredefinedMenuItem::cut(app, Some(text("Cut", "剪切")))?,
            #[cfg(not(target_os = "linux"))]
            &PredefinedMenuItem::copy(app, Some(text("Copy", "复制")))?,
            #[cfg(not(target_os = "linux"))]
            &PredefinedMenuItem::paste(app, Some(text("Paste", "粘贴")))?,
            #[cfg(not(target_os = "linux"))]
            &PredefinedMenuItem::select_all(app, Some(text("Select All", "全选")))?,
        ],
    )?;
    let view = Submenu::with_items(
        app,
        text("View", "视图"),
        true,
        &[&item(
            "app-fullscreen",
            "Toggle Full Screen",
            "切换全屏",
            None,
        )?],
    )?;
    let window = Submenu::with_id_and_items(
        app,
        WINDOW_SUBMENU_ID,
        text("Window", "窗口"),
        true,
        &[
            &item("app-minimize", "Minimize", "最小化", Some("CmdOrCtrl+M"))?,
            &item("app-zoom", "Zoom", "缩放窗口", None)?,
            &item("app-show", "Show Window", "显示窗口", None)?,
        ],
    )?;
    let help = Submenu::with_id_and_items(
        app,
        HELP_SUBMENU_ID,
        text("Help", "帮助"),
        true,
        &[
            &item("keys", "Keyboard Shortcuts…", "快捷键…", None)?,
            &item("diagnostics", "Export Diagnostics…", "导出诊断信息…", None)?,
        ],
    )?;
    #[cfg(target_os = "macos")]
    {
        window.set_as_windows_menu_for_nsapp()?;
        help.set_as_help_menu_for_nsapp()?;
    }
    Menu::with_items(app, &[&application, &file, &edit, &view, &window, &help])
}

pub fn init_menu(app: &AppHandle) -> tauri::Result<()> {
    let locale = *app.state::<Shell>().locale.lock().unwrap();
    app.set_menu(application_menu(app, locale)?)?;
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn watch_webview(app: &AppHandle) -> tauri::Result<()> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    use webkit2gtk::WebViewExt;

    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    let parent = window.clone();
    let app = app.clone();
    window.with_webview(move |view| {
        view.inner().connect_web_process_terminated(move |_, reason| {
            if exit_approved(&app) { return; }
            eprintln!("Web view terminated: {reason:?}");
            app.state::<crate::remote::Requests>().cancel_all();
            app.state::<Shell>().pending_toggle.store(false, Ordering::SeqCst);
            let stopping = tauri::async_runtime::spawn_blocking({
                let app = app.clone();
                move || crate::voice::voice_stop(app)
            });
            let locale = *app.state::<Shell>().locale.lock().unwrap();
            let app = app.clone();
            let window = parent.clone();
            app.dialog()
                .message(locale.text(
                    "The interface stopped unexpectedly. Listening is stopping. You can reopen the last saved meeting; changes that were not saved may be lost.",
                    "界面意外停止，正在停止收音。可以恢复上次保存的会议；尚未保存的修改可能已经丢失。",
                ))
                .title(locale.text("Restore meeting", "恢复会议"))
                .kind(MessageDialogKind::Warning)
                .parent(&window)
                .buttons(MessageDialogButtons::OkCancelCustom(
                    locale.text("Reload saved meeting", "恢复已保存的会议").into(),
                    locale.text("Quit", "退出").into(),
                ))
                .show(move |reload| {
                    tauri::async_runtime::spawn(async move {
                        let stopped = stopping.await.map_err(|error| error.to_string());
                        if !reload {
                            app.state::<Shell>().exit.lock().unwrap().approved = true;
                            app.exit(0);
                            return;
                        }
                        let restored = stopped.and_then(|()| {
                            app.state::<Shell>().exit.lock().unwrap().restore_view();
                            app.state::<Shell>().pending_toggle.store(false, Ordering::SeqCst);
                            window.reload().map_err(|error| error.to_string())
                        });
                        if let Err(error) = restored {
                            app.dialog().message(format!("{}\n{error}", locale.text(
                                "The interface could not reopen. Quit and start the app again. Saved meetings are unchanged.",
                                "无法重新打开界面。请退出后重新启动应用，已保存的会议不会被修改。",
                            )))
                            .title(locale.text("Could not restore meeting", "无法恢复会议"))
                            .kind(MessageDialogKind::Error)
                            .buttons(MessageDialogButtons::OkCustom(locale.text("Quit", "退出").into()))
                            .show(move |_| {
                                app.state::<Shell>().exit.lock().unwrap().approved = true;
                                app.exit(0);
                            });
                        }
                    });
                });
        });
    })
}

pub fn menu_action(app: &AppHandle, id: &str) {
    match id {
        #[cfg(target_os = "linux")]
        command @ ("Cut" | "Copy" | "Paste") => {
            use webkit2gtk::WebViewExt;
            if let Some(window) = app.get_webview_window("main") {
                let command = command.to_owned();
                if let Err(error) = window.with_webview(move |webview| {
                    webview.inner().execute_editing_command(&command);
                }) {
                    eprintln!("Edit menu action failed: {error}");
                }
            }
        }
        "app-quit" => request_exit(app, ExitReason::Quit),
        "app-close" => request_exit(app, ExitReason::Close),
        "app-show" => show_window(app),
        "app-minimize" | "app-zoom" | "app-fullscreen" => {
            if let Some(window) = app.get_webview_window("main") {
                let result = match id {
                    "app-minimize" => window.minimize(),
                    "app-zoom" => window.is_maximized().and_then(|maximized| {
                        if maximized {
                            window.unmaximize()
                        } else {
                            window.maximize()
                        }
                    }),
                    _ => window
                        .is_fullscreen()
                        .and_then(|fullscreen| window.set_fullscreen(!fullscreen)),
                };
                if let Err(error) = result {
                    eprintln!("Window menu action failed: {error}");
                }
            }
        }
        "settings" | "check-updates" | "new-meeting" | "meetings" | "import" | "export"
        | "keys" | "diagnostics" | "undo" | "redo" | "select-all" => {
            let _ = app.emit("shell-menu-action", id);
        }
        _ => {}
    }
}

#[tauri::command]
pub fn shell_set_locale(app: AppHandle, locale: String) -> Result<(), String> {
    let locale = match locale.as_str() {
        "en" => Locale::En,
        "zh" => Locale::Zh,
        _ => return Err("Unsupported menu language".into()),
    };
    app.set_menu(application_menu(&app, locale).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    *app.state::<Shell>().locale.lock().unwrap() = locale;
    update_tray_text(&app);
    Ok(())
}

pub const TOGGLE_ARG: &str = "--toggle-mic";

pub fn toggle(app: &AppHandle) {
    app.state::<Shell>().queue_toggle();
    let _ = app.emit("mic-toggle", ());
}

#[tauri::command]
pub fn shell_take_toggle(app: AppHandle) -> bool {
    app.state::<Shell>().take_toggle()
}

pub fn shortcut_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _, event| {
            if event.state == ShortcutState::Pressed {
                toggle(app);
            }
        })
        .build()
}

/// 换全局快捷键；空的就是关掉。被别的程序占了会返回错误
#[tauri::command]
pub fn shortcut_set(app: AppHandle, accel: Option<String>) -> Result<(), String> {
    let accel = accel
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let next = accel
        .map(|value| {
            value
                .parse::<Shortcut>()
                .map_err(|error| format!("Couldn't register shortcut {value}: {error}"))
        })
        .transpose()?;
    let state = app.state::<Shell>();
    let mut current = state.shortcut.lock().unwrap();
    if *current == next {
        return Ok(());
    }
    let gs = app.global_shortcut();
    // Keep the working binding until its replacement is accepted by the OS.
    if let Some(shortcut) = next {
        gs.register(shortcut)
            .map_err(|error| format!("Couldn't register shortcut {}: {error}", accel.unwrap()))?;
    }
    if let Some(previous) = *current {
        if let Err(error) = gs.unregister(previous) {
            if let Some(shortcut) = next {
                gs.unregister(shortcut).map_err(|rollback| format!("Couldn't remove the previous shortcut: {error}; couldn't undo the new shortcut: {rollback}"))?;
            }
            return Err(format!("Couldn't remove the previous shortcut: {error}"));
        }
    }
    *current = next;
    Ok(())
}

/// Wayland 下全局快捷键收不到应用外的按键，设置里要提示
#[tauri::command]
pub fn session_wayland() -> bool {
    cfg!(target_os = "linux")
        && (std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s == "wayland"))
}

#[tauri::command]
pub fn shell_show_window(app: AppHandle) {
    show_window(&app);
}

pub fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

// —— 托盘 ——

pub struct Tray {
    icon: TrayIcon,
    item: MenuItem<tauri::Wry>,
    show: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
    idle: Image<'static>,
    live: Image<'static>,
}

/// 收音时右下角加一个红点
fn with_dot(base: &Image<'_>) -> Image<'static> {
    let (w, h) = (base.width() as i32, base.height() as i32);
    let mut px = base.rgba().to_vec();
    let r = (w.min(h) as f32 * 0.3).max(3.0);
    let (cx, cy) = (w as f32 - r - 0.5, h as f32 - r - 0.5);
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
            if d <= r {
                let i = ((y * w + x) * 4) as usize;
                px[i..i + 4].copy_from_slice(&[0xe0, 0x31, 0x31, 0xff]);
            }
        }
    }
    Image::new_owned(px, w as u32, h as u32)
}

/// Linux 上托盘靠 appindicator 库，缺了 tauri 会直接崩，所以先看看有没有
#[cfg(target_os = "linux")]
fn tray_available() -> bool {
    ["libayatana-appindicator3.so.1\0", "libappindicator3.so.1\0"]
        .iter()
        .any(|name| {
            // SAFETY: 名字以 \0 结尾；只查库在不在，打开后马上关掉
            unsafe {
                let h = libc::dlopen(name.as_ptr().cast(), libc::RTLD_LAZY);
                if h.is_null() {
                    false
                } else {
                    libc::dlclose(h);
                    true
                }
            }
        })
}

#[cfg(not(target_os = "linux"))]
fn tray_available() -> bool {
    true
}

pub fn init_tray(app: &AppHandle) -> tauri::Result<()> {
    if !tray_available() {
        return Ok(());
    }
    let idle = Image::from_bytes(include_bytes!("../icons/32x32.png"))?.to_owned();
    let live = with_dot(&idle);
    let item = MenuItem::with_id(app, "tray-mic", "Start listening", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "tray-show", "Show window", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray-quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&item, &show, &quit])?;
    let icon = TrayIconBuilder::new()
        .icon(idle.clone())
        .tooltip("boardcast")
        .menu(&menu)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "tray-mic" => toggle(app),
            "tray-show" => show_window(app),
            "tray-quit" => request_exit(app, ExitReason::Quit),
            _ => {}
        })
        .build(app)?;
    app.manage(Tray {
        icon,
        item,
        show,
        quit,
        idle,
        live,
    });
    update_tray_text(app);
    Ok(())
}

/// 收音状态变了：托盘图标和菜单跟着变
pub fn set_listening(app: &AppHandle, on: bool) {
    app.state::<Shell>().listening.store(on, Ordering::Relaxed);
    let Some(t) = app.try_state::<Tray>() else {
        return;
    };
    let _ = t
        .icon
        .set_icon(Some(if on { t.live.clone() } else { t.idle.clone() }));
    update_tray_text(app);
}

fn update_tray_text(app: &AppHandle) {
    let Some(tray) = app.try_state::<Tray>() else {
        return;
    };
    let state = app.state::<Shell>();
    let locale = *state.locale.lock().unwrap();
    let on = state.listening.load(Ordering::Relaxed);
    let _ = tray.item.set_text(if on {
        locale.text("Stop listening", "停止收音")
    } else {
        locale.text("Start listening", "开始收音")
    });
    let _ = tray.show.set_text(locale.text("Show window", "显示窗口"));
    let _ = tray.quit.set_text(locale.text("Quit", "退出"));
}

#[cfg(test)]
mod tests {
    use super::{ExitGate, ExitReason, Shell};

    #[test]
    fn restored_view_rejects_old_exit_tokens_and_can_request_a_fresh_exit() {
        let mut gate = ExitGate::default();
        let old = gate.request(ExitReason::Quit).unwrap();
        gate.restore_view();
        assert!(gate.pending.is_none());
        assert!(!gate.approved);
        assert!(gate.resolve(old.id, true).is_none());
        let next = gate.request(ExitReason::Close).unwrap();
        assert_ne!(old.id, next.id);
        assert!(gate.resolve(old.id, true).is_none());
        assert_eq!(gate.resolve(next.id, true), Some(next));
    }

    #[test]
    fn startup_toggle_is_consumed_once_and_two_requests_cancel() {
        let shell = Shell::default();
        assert!(!shell.take_toggle());
        shell.queue_toggle();
        assert!(shell.take_toggle());
        assert!(!shell.take_toggle());
        shell.queue_toggle();
        shell.queue_toggle();
        assert!(!shell.take_toggle());
    }

    #[test]
    fn exiting_discards_capture_requests_even_if_exit_is_later_cancelled() {
        let shell = Shell::default();
        shell.queue_toggle();
        let exit = shell.request_exit(ExitReason::Quit).unwrap();
        shell.exit.lock().unwrap().resolve(exit.id, false);
        assert!(
            !shell.take_toggle(),
            "quit must discard an unconsumed startup request"
        );
        let exit = shell.request_exit(ExitReason::Quit).unwrap();
        shell.queue_toggle();
        shell.exit.lock().unwrap().resolve(exit.id, false);
        assert!(
            !shell.take_toggle(),
            "a request received while exiting must not run after cancellation"
        );
        shell.queue_toggle();
        assert!(shell.take_toggle());
        let exit = shell.request_exit(ExitReason::Quit).unwrap();
        shell.exit.lock().unwrap().resolve(exit.id, true);
        shell.queue_toggle();
        assert!(!shell.take_toggle());
    }

    #[test]
    fn exit_requests_deduplicate_until_the_current_token_is_resolved() {
        let mut gate = ExitGate::default();
        let first = gate.request(ExitReason::Close).unwrap();
        assert!(gate.request(ExitReason::Quit).is_none());
        assert!(!gate.approved);
        assert!(gate.resolve(first.id + 1, true).is_none());
        assert_eq!(gate.pending, Some(first));
        assert_eq!(gate.resolve(first.id, false), Some(first));
        assert!(!gate.approved);
        assert!(gate.pending.is_none());
    }

    #[test]
    fn failed_save_allows_retry_but_an_old_success_cannot_exit() {
        let mut gate = ExitGate::default();
        let first = gate.request(ExitReason::Quit).unwrap();
        gate.resolve(first.id, false);
        let retry = gate.request(ExitReason::Quit).unwrap();
        assert_ne!(first.id, retry.id);
        assert!(gate.resolve(first.id, true).is_none());
        assert!(!gate.approved);
        assert_eq!(gate.resolve(retry.id, true), Some(retry));
        assert!(gate.approved);
        assert!(gate.resolve(retry.id, true).is_none());
        assert!(gate.request(ExitReason::Close).is_none());
    }
}
