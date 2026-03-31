use std::ffi::OsString;

/// Secret-handling test knobs must never affect release binaries.
///
/// We allow them in test and debug builds so integration tests can stay
/// hermetic and developers can reproduce storage backends locally.
pub fn var(key: &str) -> Option<OsString> {
    if cfg!(any(test, debug_assertions)) {
        std::env::var_os(key)
    } else {
        None
    }
}

pub fn string(key: &str) -> Option<String> {
    var(key).and_then(|value| value.into_string().ok())
}

/// RAII guard that sets an environment variable for the duration of a test and
/// restores (or removes) it when dropped.
///
/// # Safety
///
/// Tests that use this guard must hold `crate::SPAWN_LOCK` while the guard is
/// live to prevent concurrent threads from observing the mutated environment.
#[cfg(test)]
pub(crate) struct EnvVarGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

#[cfg(test)]
impl EnvVarGuard {
    pub(crate) fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: guarded by SPAWN_LOCK in callers.
        unsafe { std::env::set_var(key, value) }
        Self { key, previous }
    }
}

#[cfg(test)]
impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        // SAFETY: guarded by SPAWN_LOCK in callers.
        match &self.previous {
            Some(value) => unsafe { std::env::set_var(self.key, value) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}
