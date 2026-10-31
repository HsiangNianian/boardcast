//! 应用不在前台时控制收音：全局快捷键、托盘、`boardcast --toggle-mic`。
//! 这里只发一个 mic-toggle 事件，开始还是停止由前端决定（它知道模型装好没有、选的哪个麦克风）。
//! Wayland 不让应用监听全局按键，那边把 `boardcast --toggle-mic` 绑到系统快捷键上。

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

pub const TOGGLE_ARG: &str = "--toggle-mic";

pub fn toggle(app: &AppHandle) {
    let _ = app.emit("mic-toggle", ());
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
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| e.to_string())?;
    match accel.as_deref().map(str::trim) {
        Some(a) if !a.is_empty() => gs
            .register(a)
            .map_err(|e| format!("注册不了 {a}：{e}")),
        _ => Ok(()),
    }
}

/// Wayland 下全局快捷键收不到应用外的按键，设置里要提示
#[tauri::command]
pub fn session_wayland() -> bool {
    cfg!(target_os = "linux")
        && (std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s == "wayland"))
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
    let item = MenuItem::with_id(app, "mic", "开始收音", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&item, &show, &quit])?;
    let icon = TrayIconBuilder::new()
        .icon(idle.clone())
        .tooltip("boardcast")
        .menu(&menu)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "mic" => toggle(app),
            "show" => show_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    app.manage(Tray { icon, item, idle, live });
    Ok(())
}

/// 收音状态变了：托盘图标和菜单跟着变
pub fn set_listening(app: &AppHandle, on: bool) {
    let Some(t) = app.try_state::<Tray>() else {
        return;
    };
    let _ = t.icon.set_icon(Some(if on { t.live.clone() } else { t.idle.clone() }));
    let _ = t.item.set_text(if on { "停止收音" } else { "开始收音" });
}
