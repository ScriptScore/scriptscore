// SPDX-License-Identifier: AGPL-3.0-only
use std::ffi::{OsStr, OsString};
use std::sync::{Mutex, MutexGuard};

pub static ENV_VAR_LOCK: Mutex<()> = Mutex::new(());

pub fn lock_env_vars() -> MutexGuard<'static, ()> {
    ENV_VAR_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

pub struct EnvVarGuard {
    key: &'static str,
    original: Option<OsString>,
}

impl EnvVarGuard {
    pub fn set(key: &'static str, value: impl AsRef<OsStr>) -> Self {
        let original = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(original) = self.original.as_ref() {
            std::env::set_var(self.key, original);
        } else {
            std::env::remove_var(self.key);
        }
    }
}
