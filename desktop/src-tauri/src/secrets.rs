// SPDX-License-Identifier: AGPL-3.0-only
//! App-level secrets stored outside `scriptscore.db` (OS keychain / credential store).

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use rand::RngCore;

use crate::errors::{HostError, HostResult};
use crate::models::AppSettings;

const KEYRING_SERVICE: &str = "scriptscore-desktop";
const BINDING_HMAC_KEY: &str = "lms_binding_hmac_secret_v1";

static BINDING_HMAC_SECRET_CACHE: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// 32-byte secret for LMS binding tokens; created on first use. Cached in-process so repeated
/// keychain reads cannot yield inconsistent binding tokens within one app run.
pub fn binding_hmac_secret_bytes(settings: &AppSettings) -> HostResult<Vec<u8>> {
    let mut slot = BINDING_HMAC_SECRET_CACHE
        .lock()
        .map_err(|_| HostError::Project("binding HMAC secret cache lock poisoned".into()))?;
    if let Some(key) = slot.as_ref() {
        if plaintext_binding_secret_fallback_enabled(settings) {
            ensure_binding_hmac_secret_file_present(key)?;
        }
        return Ok(key.clone());
    }
    let key = load_binding_hmac_secret(settings)?;
    *slot = Some(key.clone());
    Ok(key)
}

#[cfg(test)]
pub fn __test_set_binding_hmac_secret_bytes(bytes: Vec<u8>) {
    let mut slot = BINDING_HMAC_SECRET_CACHE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    *slot = Some(bytes);
}

#[cfg(test)]
pub fn __test_clear_binding_hmac_secret_cache() {
    let mut slot = BINDING_HMAC_SECRET_CACHE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    *slot = None;
}

fn plaintext_binding_secret_fallback_enabled(settings: &AppSettings) -> bool {
    settings.lms_binding_secret_plaintext_fallback
}

fn load_binding_hmac_secret(settings: &AppSettings) -> HostResult<Vec<u8>> {
    let allow_plaintext_fallback = plaintext_binding_secret_fallback_enabled(settings);
    if let Some(key) = load_existing_binding_hmac_secret(allow_plaintext_fallback)? {
        return Ok(key);
    }
    let key = generate_binding_hmac_secret();
    persist_new_binding_hmac_secret(&key, allow_plaintext_fallback)?;
    Ok(key)
}

fn load_existing_binding_hmac_secret(
    allow_plaintext_fallback: bool,
) -> HostResult<Option<Vec<u8>>> {
    if let Some(key) = load_binding_hmac_secret_from_primary_store(allow_plaintext_fallback)? {
        return Ok(Some(key));
    }
    if let Some(key) = load_binding_hmac_secret_from_file()? {
        import_file_secret_into_keyring(&key, allow_plaintext_fallback)?;
        return Ok(Some(key));
    }
    Ok(None)
}

fn load_binding_hmac_secret_from_primary_store(
    allow_plaintext_fallback: bool,
) -> HostResult<Option<Vec<u8>>> {
    match load_binding_hmac_secret_from_keyring() {
        Ok(Some(key)) => {
            maybe_persist_binding_hmac_secret_to_file(&key, allow_plaintext_fallback)?;
            Ok(Some(key))
        }
        Ok(None) => Ok(None),
        Err(_) if allow_plaintext_fallback => Ok(None),
        Err(err) => Err(err),
    }
}

fn maybe_persist_binding_hmac_secret_to_file(
    key: &[u8],
    allow_plaintext_fallback: bool,
) -> HostResult<()> {
    if allow_plaintext_fallback {
        persist_binding_hmac_secret_to_file(key)?;
    }
    Ok(())
}

fn import_file_secret_into_keyring(key: &[u8], allow_plaintext_fallback: bool) -> HostResult<()> {
    if allow_plaintext_fallback {
        let _ = persist_binding_hmac_secret_to_keyring(key);
        return Ok(());
    }
    persist_binding_hmac_secret_to_keyring(key)
}

fn generate_binding_hmac_secret() -> Vec<u8> {
    let mut key = vec![0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

fn persist_new_binding_hmac_secret(key: &[u8], allow_plaintext_fallback: bool) -> HostResult<()> {
    if allow_plaintext_fallback {
        persist_binding_hmac_secret_to_file(key)?;
        let _ = persist_binding_hmac_secret_to_keyring(key);
        return Ok(());
    }
    persist_binding_hmac_secret_to_keyring(key)
}

fn ensure_binding_hmac_secret_file_present(key: &[u8]) -> HostResult<()> {
    let path = binding_hmac_secret_file_path()?;
    if path.is_file() {
        return Ok(());
    }
    persist_binding_hmac_secret_to_file_path(key, &path)
}

fn load_binding_hmac_secret_from_keyring() -> HostResult<Option<Vec<u8>>> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, BINDING_HMAC_KEY)
        .map_err(|err| HostError::Project(format!("Keyring entry failed: {err}")))?;
    match entry.get_password() {
        Ok(hex_str) => hex::decode(hex_str.trim()).map(Some).map_err(|err| {
            HostError::Project(format!("Stored HMAC secret is not valid hex: {err}"))
        }),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(HostError::Project(format!(
            "Could not read HMAC secret from keychain: {err}"
        ))),
    }
}

fn persist_binding_hmac_secret_to_keyring(key: &[u8]) -> HostResult<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, BINDING_HMAC_KEY)
        .map_err(|err| HostError::Project(format!("Keyring entry failed: {err}")))?;
    let encoded = hex::encode(key);
    entry.set_password(&encoded).map_err(|err| {
        HostError::Project(format!("Could not store HMAC secret in OS keychain: {err}"))
    })?;
    Ok(())
}

fn binding_hmac_secret_file_path() -> HostResult<PathBuf> {
    if let Ok(explicit) = std::env::var("SCRIPTSCORE_BINDING_SECRET_PATH") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    let base_dir = dirs::config_local_dir()
        .or_else(dirs::config_dir)
        .ok_or_else(|| {
            HostError::Project(
                "Could not resolve a local config directory for the LMS binding secret.".into(),
            )
        })?;
    Ok(base_dir
        .join("scriptscore-desktop")
        .join("secrets")
        .join(format!("{BINDING_HMAC_KEY}.hex")))
}

fn load_binding_hmac_secret_from_file() -> HostResult<Option<Vec<u8>>> {
    let path = binding_hmac_secret_file_path()?;
    load_binding_hmac_secret_from_file_path(&path)
}

fn load_binding_hmac_secret_from_file_path(path: &PathBuf) -> HostResult<Option<Vec<u8>>> {
    if !path.is_file() {
        return Ok(None);
    }
    let encoded = fs::read_to_string(path)?;
    let key = hex::decode(encoded.trim()).map_err(|err| {
        HostError::Project(format!(
            "Stored LMS binding secret file is not valid hex ({}): {err}",
            path.display()
        ))
    })?;
    Ok(Some(key))
}

fn persist_binding_hmac_secret_to_file(key: &[u8]) -> HostResult<()> {
    let path = binding_hmac_secret_file_path()?;
    persist_binding_hmac_secret_to_file_path(key, &path)
}

fn persist_binding_hmac_secret_to_file_path(key: &[u8], path: &PathBuf) -> HostResult<()> {
    let parent = path.parent().ok_or_else(|| {
        HostError::Project("Could not determine the LMS binding secret directory.".into())
    })?;
    fs::create_dir_all(parent)?;
    fs::write(path, hex::encode(key))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_hmac_secret_file_round_trips() {
        let temp_dir =
            std::env::temp_dir().join(format!("scriptscore-secret-test-{}", std::process::id()));
        let path = temp_dir.join("binding.hex");
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(&temp_dir);
        let first = vec![0x55; 32];
        persist_binding_hmac_secret_to_file_path(&first, &path).unwrap();
        let second = load_binding_hmac_secret_from_file_path(&path)
            .unwrap()
            .unwrap();
        assert_eq!(first.len(), 32);
        assert_eq!(first, second);

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
