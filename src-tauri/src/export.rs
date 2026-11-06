//! 导出：弹系统的「另存为」对话框，把前端生成好的文件写下去。
//! WebView 里的 <a download> 在 Linux（WebKitGTK）上不一定管用，所以桌面版走这里。

use base64::Engine as _;
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

#[derive(Default)]
pub struct SavedExports(Mutex<HashSet<PathBuf>>);

impl SavedExports {
    fn remember(&self, path: &Path) -> Result<PathBuf, String> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("Saved file could not be located: {e}"))?;
        self.0
            .lock()
            .map_err(|e| e.to_string())?
            .insert(path.clone());
        Ok(path)
    }

    fn resolve(&self, path: &str) -> Result<PathBuf, String> {
        let path = PathBuf::from(path);
        if !self.0.lock().map_err(|e| e.to_string())?.contains(&path) {
            return Err("Only files exported by this app session can be opened".into());
        }
        let actual = path
            .canonicalize()
            .map_err(|e| format!("The exported file is no longer available: {e}"))?;
        if actual != path || !actual.is_file() {
            return Err("The exported file was moved or replaced. Export it again.".into());
        }
        Ok(actual)
    }
}

fn openable(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("png" | "svg" | "md" | "txt" | "excalidraw" | "html" | "pdf" | "json" | "jsonl")
    )
}

fn open_command(path: &Path, reveal: bool) -> Result<Command, String> {
    if !reveal && !openable(path) {
        return Err(
            "This file type cannot be opened automatically. Show its folder instead.".into(),
        );
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("/usr/bin/open");
        if reveal {
            command.arg("-R");
        }
        command.arg(path);
        Ok(command)
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        let target = if reveal {
            path.parent()
                .ok_or("The exported file has no parent folder")?
        } else {
            path
        };
        let mut command = Command::new(if cfg!(target_os = "windows") {
            "explorer.exe"
        } else {
            "xdg-open"
        });
        command.arg(target);
        Ok(command)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    Err("Opening exported files is unavailable on this platform".into())
}

async fn handoff(path: &Path, reveal: bool) -> Result<(), String> {
    let mut command = open_command(path, reveal)?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not request opening the export: {e}"))?;
    let waiting = tauri::async_runtime::spawn_blocking(move || child.wait());
    // Some default apps keep the launcher alive; a handoff is not proof that their window opened.
    match tokio::time::timeout(Duration::from_secs(2), waiting).await {
        Ok(result) => {
            let status = result
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(
                    "The system could not open this export. Try showing its folder.".into(),
                );
            }
            Ok(())
        }
        Err(_) => Ok(()),
    }
}

#[tauri::command]
pub async fn export_open(
    exports: tauri::State<'_, SavedExports>,
    path: String,
) -> Result<(), String> {
    handoff(&exports.resolve(&path)?, false).await
}

#[tauri::command]
pub async fn export_reveal(
    exports: tauri::State<'_, SavedExports>,
    path: String,
) -> Result<(), String> {
    handoff(&exports.resolve(&path)?, true).await
}

fn save_export(path: &Path, bytes: &[u8]) -> io::Result<()> {
    // Preserve an explicitly selected symlink and replace its destination instead.
    let path = if path
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        path.canonicalize()?
    } else {
        path.to_path_buf()
    };
    let previous = match fs::metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Export target is not a regular file",
                ));
            }
            if metadata.permissions().readonly() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Permission denied: export target is read-only",
                ));
            }
            // Replacing a directory entry must not bypass a read-only target.
            Some(OpenOptions::new().write(true).open(&path)?)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let (temp, mut file) = loop {
        let temp = path.parent().unwrap_or(Path::new(".")).join(format!(
            ".boardcast-export-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temp) {
            Ok(file) => break (temp, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let result = (|| {
        file.write_all(bytes)?;
        if let Some(previous) = previous {
            let metadata = previous.metadata()?;
            #[cfg(target_os = "linux")]
            {
                use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
                let current = file.metadata()?;
                if (current.uid(), current.gid()) != (metadata.uid(), metadata.gid())
                    && unsafe { libc::fchown(file.as_raw_fd(), metadata.uid(), metadata.gid()) }
                        != 0
                {
                    return Err(io::Error::last_os_error());
                }
                // Mode group bits can be an ACL mask; copying them alone can grant new readers.
                let name = c"system.posix_acl_access";
                let mut acl = vec![0u8; 65536];
                let size = unsafe {
                    libc::fgetxattr(
                        previous.as_raw_fd(),
                        name.as_ptr(),
                        acl.as_mut_ptr().cast(),
                        acl.len(),
                    )
                };
                if size >= 0 {
                    if unsafe {
                        libc::fsetxattr(
                            file.as_raw_fd(),
                            name.as_ptr(),
                            acl.as_ptr().cast(),
                            size as usize,
                            0,
                        )
                    } != 0
                    {
                        return Err(io::Error::last_os_error());
                    }
                } else {
                    let error = io::Error::last_os_error();
                    match error.raw_os_error() {
                        Some(libc::ENODATA) => {
                            // Discard a default ACL inherited by the temporary file.
                            if unsafe { libc::fremovexattr(file.as_raw_fd(), name.as_ptr()) } != 0 {
                                let error = io::Error::last_os_error();
                                if error.raw_os_error() != Some(libc::ENODATA) {
                                    return Err(error);
                                }
                            }
                        }
                        Some(libc::ENOTSUP) => {}
                        _ => return Err(error),
                    }
                }
            }
            file.set_permissions(metadata.permissions())?;
        }
        file.sync_all()?;
        drop(file);
        // shortcut: no parent-directory sync; add it for power-loss durability.
        fs::rename(&temp, &path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// 用户取消时返回 None，保存成功返回写到的路径
#[tauri::command]
pub async fn export_save(
    app: AppHandle,
    exports: tauri::State<'_, SavedExports>,
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
    save_export(&path, &bytes).map_err(|e| format!("Save failed: {e}"))?;
    Ok(Some(exports.remember(&path)?.display().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_replace_whole_files_and_concurrent_writes_do_not_mix() {
        let folder =
            std::env::temp_dir().join(format!("boardcast-export-atomic-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("meeting notes.html");
        save_export(&path, b"original").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"original");
        let threads: Vec<_> = (0..8)
            .map(|value| {
                let path = path.clone();
                std::thread::spawn(move || save_export(&path, &vec![value; 128 * 1024]).unwrap())
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let bytes = fs::read(&path).unwrap();
        assert_eq!(bytes.len(), 128 * 1024);
        assert!(bytes[0] < 8 && bytes.iter().all(|b| *b == bytes[0]));
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 1);
        fs::remove_dir_all(folder).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn exports_preserve_symlinks_permissions_and_read_only_targets() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let folder = std::env::temp_dir().join(format!(
            "boardcast-export-permissions-{}",
            std::process::id()
        ));
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("original.md");
        let link = folder.join("selected.md");
        fs::write(&path, b"old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        symlink(&path, &link).unwrap();
        save_export(&link, b"complete replacement").unwrap();
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(fs::read(&path).unwrap(), b"complete replacement");
        assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o640);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        assert_eq!(
            save_export(&link, b"must not replace").unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(fs::read(&path).unwrap(), b"complete replacement");
        fs::remove_file(&path).unwrap();
        assert!(save_export(&link, b"must not replace dangling link").is_err());
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert!(!path.exists());
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 1);
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    #[ignore = "requires BOARDCAST_EXPORT_FULL_VOLUME pointing to a private nearly-full tmpfs"]
    fn full_volume_preserves_old_export_and_leaves_no_partial_new_export() {
        let folder = PathBuf::from(std::env::var("BOARDCAST_EXPORT_FULL_VOLUME").unwrap());
        assert_eq!(folder.file_name().unwrap(), "volume");
        assert!(folder
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("boardcast-g2-export-atomic-"));
        let existing = folder.join("existing.html");
        let fresh = folder.join("fresh.html");
        assert!(!fresh.exists());
        let before = fs::read(&existing).unwrap();
        let entries = fs::read_dir(&folder).unwrap().count();
        for target in [&existing, &fresh] {
            let error = save_export(target, &vec![b'X'; 4 * 1024 * 1024]).unwrap_err();
            #[cfg(target_os = "linux")]
            assert_eq!(error.raw_os_error(), Some(libc::ENOSPC));
            assert_eq!(fs::read(&existing).unwrap(), before);
            assert!(!fresh.exists());
            assert_eq!(fs::read_dir(&folder).unwrap().count(), entries);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn exports_preserve_access_acl_and_remove_unwanted_inherited_acl() {
        use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
        let folder =
            std::env::temp_dir().join(format!("boardcast-export-acl-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("restricted.html");
        let original = fs::File::create(&path).unwrap();
        // Use a mapped uid so this ACL fixture also works in a user namespace.
        let mut acl = 2u32.to_le_bytes().to_vec();
        for (tag, permissions, id) in [
            (1u16, 6u16, u32::MAX),
            (2, 4, unsafe { libc::geteuid() }),
            (4, 0, u32::MAX),
            (16, 4, u32::MAX),
            (32, 0, u32::MAX),
        ] {
            acl.extend(tag.to_le_bytes());
            acl.extend(permissions.to_le_bytes());
            acl.extend(id.to_le_bytes());
        }
        let name = c"system.posix_acl_access";
        assert_eq!(
            unsafe {
                libc::fsetxattr(
                    original.as_raw_fd(),
                    name.as_ptr(),
                    acl.as_ptr().cast(),
                    acl.len(),
                    0,
                )
            },
            0,
            "{}",
            io::Error::last_os_error()
        );
        save_export(&path, b"complete export").unwrap();
        let replaced = fs::File::open(&path).unwrap();
        let mut actual = vec![0u8; acl.len()];
        assert_eq!(
            unsafe {
                libc::fgetxattr(
                    replaced.as_raw_fd(),
                    name.as_ptr(),
                    actual.as_mut_ptr().cast(),
                    actual.len(),
                )
            },
            acl.len() as isize
        );
        assert_eq!(actual, acl);
        assert_eq!(replaced.metadata().unwrap().mode() & 0o777, 0o640);
        assert_eq!(
            unsafe { libc::fremovexattr(replaced.as_raw_fd(), name.as_ptr()) },
            0
        );
        let directory = fs::File::open(&folder).unwrap();
        assert_eq!(
            unsafe {
                libc::fsetxattr(
                    directory.as_raw_fd(),
                    c"system.posix_acl_default".as_ptr(),
                    acl.as_ptr().cast(),
                    acl.len(),
                    0,
                )
            },
            0
        );
        save_export(&path, b"another complete export").unwrap();
        let replaced = fs::File::open(&path).unwrap();
        assert_eq!(
            unsafe {
                libc::fgetxattr(
                    replaced.as_raw_fd(),
                    name.as_ptr(),
                    actual.as_mut_ptr().cast(),
                    actual.len(),
                )
            },
            -1
        );
        assert_eq!(
            io::Error::last_os_error().raw_os_error(),
            Some(libc::ENODATA)
        );
        assert_eq!(replaced.metadata().unwrap().mode() & 0o777, 0o640);
        assert_eq!(fs::read(&path).unwrap(), b"another complete export");
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 1);
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn only_recorded_exports_can_be_opened_and_missing_files_fail() {
        let folder = std::env::temp_dir().join(format!("boardcast-export-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("meeting notes.md");
        std::fs::write(&path, b"fixture").unwrap();
        let exports = SavedExports::default();
        assert!(exports.resolve(path.to_str().unwrap()).is_err());
        let canonical = exports.remember(&path).unwrap();
        assert_eq!(
            exports.resolve(canonical.to_str().unwrap()).unwrap(),
            canonical
        );
        std::fs::remove_file(&path).unwrap();
        assert!(exports.resolve(canonical.to_str().unwrap()).is_err());
        std::fs::remove_dir(folder).unwrap();
    }

    #[test]
    fn launcher_keeps_paths_as_arguments_and_never_opens_executables() {
        let path = Path::new("/tmp/meeting $(not-a-command) & notes.md");
        let command = open_command(path, false).unwrap();
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![path.as_os_str()]
        );
        assert!(open_command(Path::new("/tmp/script.exe"), false).is_err());
        assert!(open_command(Path::new("/tmp/script.sh"), false).is_err());
        #[cfg(not(target_os = "macos"))]
        assert_eq!(
            open_command(path, true)
                .unwrap()
                .get_args()
                .collect::<Vec<_>>(),
            vec![Path::new("/tmp").as_os_str()]
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_export_replaced_with_a_symlink_cannot_open_another_file() {
        let folder =
            std::env::temp_dir().join(format!("boardcast-export-link-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("export.md");
        let other = folder.join("unexported.md");
        std::fs::write(&path, b"exported").unwrap();
        std::fs::write(&other, b"other").unwrap();
        let exports = SavedExports::default();
        let canonical = exports.remember(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&other, &path).unwrap();
        assert!(exports.resolve(canonical.to_str().unwrap()).is_err());
        std::fs::remove_dir_all(folder).unwrap();
    }
}
