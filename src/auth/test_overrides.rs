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
