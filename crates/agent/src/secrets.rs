//! Where the API key and the proxy password are kept: the system keychain (the macOS Keychain, the
//! Windows Credential Manager, the Secret Service of GNOME Keyring or KWallet on Linux), never the
//! settings file.
//!
//! Where there is no keychain, as on a Linux desktop without a Secret Service, they go to `secrets.json`
//! beside the settings, readable by this user only and encrypted with ChaCha20-Poly1305 under a key
//! derived from this machine's id and the user's home folder. That keeps a copy of the file (a backup,
//! a synced folder) unreadable anywhere else; it cannot keep out a program already running as this user
//! on this machine, which is what the keychain is for.

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

/// The keychain entry's service, the app's id.
const SERVICE: &str = "app.alcyo.board";
const FILE: &str = "secrets.json";

/// Where a secret ended up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kept {
    Keychain,
    /// In the encrypted file, as no keychain was there to take it.
    File,
}

/// What was last read or written, so saving the settings again (which happens on every change of any
/// setting) does not ask the keychain each time.
static KNOWN: Mutex<Option<HashMap<String, (String, Kept)>>> = Mutex::new(None);

fn known(name: &str) -> Option<(String, Kept)> {
    KNOWN.lock().ok()?.as_ref()?.get(name).cloned()
}

fn remember(name: &str, value: &str, kept: Kept) {
    if let Ok(mut known) = KNOWN.lock() {
        known.get_or_insert_with(HashMap::new).insert(name.to_string(), (value.to_string(), kept));
    }
}

/// Where the secrets are kept now: in the file if any of them is.
pub fn kept() -> Kept {
    let file = KNOWN.lock().ok().and_then(|k| k.as_ref().map(|k| k.values().any(|(v, kept)| !v.is_empty() && *kept == Kept::File)));
    if file == Some(true) {
        Kept::File
    } else {
        Kept::Keychain
    }
}

/// The secret saved under `name`, or an empty string when there is none.
pub fn load(dir: &Path, name: &str) -> String {
    if let Some((value, _)) = known(name) {
        return value;
    }
    let (value, kept) = match keychain::get(name) {
        Ok(Some(value)) => (value, Kept::Keychain),
        // Nothing in the keychain: it may still be in the file from a time the keychain was not there.
        Ok(None) => (file_get(dir, name).unwrap_or_default(), Kept::File),
        Err(_) => (file_get(dir, name).unwrap_or_default(), Kept::File),
    };
    remember(name, &value, kept);
    value
}

/// Save `value` under `name`; an empty value removes it. The keychain is tried first; the file only
/// when there is none, and a copy left in the file is removed once the keychain has it.
pub fn store(dir: &Path, name: &str, value: &str) -> Result<Kept> {
    if let Some((known, kept)) = known(name) {
        if known == value {
            return Ok(kept);
        }
    }
    let kept = match keychain::set(name, value) {
        Ok(()) => {
            if file_get(dir, name).is_some() {
                file_set(dir, name, "")?;
            }
            Kept::Keychain
        }
        Err(_) => {
            file_set(dir, name, value)?;
            Kept::File
        }
    };
    remember(name, value, kept);
    Ok(kept)
}

/// The key the file is encrypted with: this machine and this user, never stored anywhere.
fn file_key() -> Key {
    let machine = ["/etc/machine-id", "/var/lib/dbus/machine-id"].iter().find_map(|p| fs::read_to_string(p).ok()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let machine = machine.or_else(|| std::env::var("COMPUTERNAME").ok()).or_else(|| std::env::var("HOSTNAME").ok()).unwrap_or_default();
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default();
    let mut hash = Sha256::new();
    for part in ["alcyo secrets v1", &machine, &home] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    let digest: [u8; 32] = hash.finalize().into();
    digest.into()
}

fn encrypt(name: &str, value: &str) -> Result<String> {
    let mut nonce = [0u8; 12];
    getrandom::getrandom(&mut nonce).map_err(|e| anyhow!("no randomness for the secret: {e}"))?;
    let cipher = ChaCha20Poly1305::new(&file_key());
    // The name is bound in, so one entry cannot be moved to stand in for another.
    let sealed = cipher.encrypt(Nonce::from_slice(&nonce), chacha20poly1305::aead::Payload { msg: value.as_bytes(), aad: name.as_bytes() }).map_err(|_| anyhow!("the secret could not be encrypted"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode([nonce.as_slice(), &sealed].concat()))
}

fn decrypt(name: &str, text: &str) -> Option<String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(text).ok()?;
    if bytes.len() < 12 {
        return None;
    }
    let (nonce, sealed) = bytes.split_at(12);
    let plain = ChaCha20Poly1305::new(&file_key()).decrypt(Nonce::from_slice(nonce), chacha20poly1305::aead::Payload { msg: sealed, aad: name.as_bytes() }).ok()?;
    String::from_utf8(plain).ok()
}

fn file_entries(dir: &Path) -> Map<String, Value> {
    fs::read_to_string(dir.join(FILE)).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default()
}

/// A secret from the file. One that does not decrypt (copied from another machine, or damaged) is none.
fn file_get(dir: &Path, name: &str) -> Option<String> {
    file_entries(dir).get(name)?.as_str().and_then(|t| decrypt(name, t))
}

fn file_set(dir: &Path, name: &str, value: &str) -> Result<()> {
    let mut entries = file_entries(dir);
    if value.is_empty() {
        if entries.remove(name).is_none() {
            return Ok(());
        }
    } else {
        entries.insert(name.to_string(), Value::String(encrypt(name, value)?));
    }
    if entries.is_empty() {
        let _ = fs::remove_file(dir.join(FILE));
        return Ok(());
    }
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let tmp = dir.join(format!("{FILE}.tmp"));
    let _ = fs::remove_file(&tmp);
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
    file.write_all(serde_json::to_string_pretty(&Value::Object(entries))?.as_bytes())?;
    file.sync_all()?;
    fs::rename(&tmp, dir.join(FILE)).context("saving the secrets")?;
    Ok(())
}

/// The system keychain. Tests never touch the real one: they behave as a machine without a keychain.
#[cfg(any(test, not(any(target_os = "linux", target_os = "macos", windows))))]
mod keychain {
    pub fn get(_name: &str) -> Result<Option<String>, ()> {
        Err(())
    }
    pub fn set(_name: &str, _value: &str) -> Result<(), ()> {
        Err(())
    }
}

#[cfg(all(not(test), any(target_os = "linux", target_os = "macos", windows)))]
mod keychain {
    use super::SERVICE;
    use keyring::{Entry, Error};

    /// `Ok(None)` when the keychain works but holds nothing under `name`; `Err` when there is no keychain.
    pub fn get(name: &str) -> Result<Option<String>, ()> {
        match Entry::new(SERVICE, name).and_then(|e| e.get_password()) {
            Ok(value) => Ok(Some(value)),
            Err(Error::NoEntry) => Ok(None),
            Err(_) => Err(()),
        }
    }

    pub fn set(name: &str, value: &str) -> Result<(), ()> {
        let entry = Entry::new(SERVICE, name).map_err(|_| ())?;
        if value.is_empty() {
            return match entry.delete_credential() {
                Ok(()) | Err(Error::NoEntry) => Ok(()),
                Err(_) => Err(()),
            };
        }
        entry.set_password(value).map_err(|_| ())?;
        // Read back: a Secret Service that is there but locked can accept nothing.
        match entry.get_password() {
            Ok(back) if back == value => Ok(()),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("alcyo-secrets-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn without_a_keychain_the_file_holds_the_secret_encrypted() {
        let d = dir("file");
        assert_eq!(file_get(&d, "api-key"), None);
        file_set(&d, "api-key", "sk-very-secret-123").unwrap();
        let text = fs::read_to_string(d.join(FILE)).unwrap();
        assert!(!text.contains("sk-very-secret"), "never in the clear: {text}");
        assert_eq!(file_get(&d, "api-key").as_deref(), Some("sk-very-secret-123"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(d.join(FILE)).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // Removing the last secret removes the file.
        file_set(&d, "api-key", "").unwrap();
        assert!(!d.join(FILE).exists());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn an_entry_cannot_stand_in_for_another_or_survive_damage() {
        let sealed = encrypt("api-key", "sk-1").unwrap();
        assert_eq!(decrypt("api-key", &sealed).as_deref(), Some("sk-1"));
        assert_eq!(decrypt("proxy-password", &sealed), None, "bound to its name");
        let mut bytes = base64::engine::general_purpose::STANDARD.decode(&sealed).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        assert_eq!(decrypt("api-key", &base64::engine::general_purpose::STANDARD.encode(bytes)), None);
        assert_eq!(decrypt("api-key", "not base64!"), None);
        assert_ne!(encrypt("api-key", "sk-1").unwrap(), sealed, "a fresh nonce each time");
    }

    #[test]
    fn store_and_load_go_to_the_file_when_there_is_no_keychain() {
        let d = dir("store");
        assert_eq!(store(&d, "test-secret", "p@ss").unwrap(), Kept::File);
        assert_eq!(file_get(&d, "test-secret").as_deref(), Some("p@ss"));
        assert_eq!(load(&d, "test-secret"), "p@ss");
        assert_eq!(kept(), Kept::File);
        store(&d, "test-secret", "").unwrap();
        assert_eq!(load(&d, "test-secret"), "");
        let _ = fs::remove_dir_all(d);
    }
}
