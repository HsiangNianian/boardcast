//! On Windows, put the app icon into boardcast.exe, so Explorer, the taskbar and the window title bar
//! show it. Resource 1 is also the icon GPUI loads for its windows.

use std::{env, fs, path::PathBuf};

fn main() {
    let icon = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../packaging/icons/icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let icon = icon.canonicalize().expect("packaging/icons/icon.ico is missing; run packaging/icons/make.sh");
    // Written here rather than kept beside the crate, so the icon's path is absolute and the resource
    // compiler finds it whatever directory it runs in. A canonical Windows path starts with \\?\,
    // which rc.exe does not take.
    let path = icon.display().to_string();
    let path = path.strip_prefix(r"\\?\").unwrap_or(&path).replace('\\', "\\\\");
    let rc = PathBuf::from(env::var("OUT_DIR").unwrap()).join("boardcast.rc");
    fs::write(&rc, format!("1 ICON \"{path}\"\n")).unwrap();
    embed_resource::compile(&rc, embed_resource::NONE).manifest_optional().unwrap();
}
