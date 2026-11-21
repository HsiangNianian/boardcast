//! On Windows, put the app icon into alcyo.exe, so Explorer, the taskbar and the window title bar
//! show it. Resource 1 is also the icon GPUI loads for its windows. The version resource gives the
//! name Task Manager and the file's properties show, which otherwise fall back to "alcyo.exe".

use std::{env, fs, path::PathBuf};

const NAME: &str = "Alcyo";

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
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    // FILEVERSION takes four numbers: 0.33.0 -> 0,33,0,0; anything after a '-' is dropped.
    let mut numbers: Vec<String> = version.split('-').next().unwrap().split('.').map(str::to_string).collect();
    numbers.resize(4, "0".into());
    let numbers = numbers.join(",");
    fs::write(
        &rc,
        format!(
            r#"1 ICON "{path}"

1 VERSIONINFO
FILEVERSION {numbers}
PRODUCTVERSION {numbers}
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "FileDescription", "{NAME}"
      VALUE "ProductName", "{NAME}"
      VALUE "FileVersion", "{version}"
      VALUE "ProductVersion", "{version}"
      VALUE "OriginalFilename", "alcyo.exe"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
        ),
    )
    .unwrap();
    embed_resource::compile(&rc, embed_resource::NONE).manifest_optional().unwrap();
}
