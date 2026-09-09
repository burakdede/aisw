use std::path::PathBuf;

#[cfg(not(test))]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
use std::cell::Cell;

#[cfg(not(test))]
static NON_INTERACTIVE: AtomicBool = AtomicBool::new(false);
#[cfg(not(test))]
static QUIET: AtomicBool = AtomicBool::new(false);
#[cfg(not(test))]
static OUTPUT_MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
#[cfg(not(test))]
static COMMAND: std::sync::Mutex<Option<&'static str>> = std::sync::Mutex::new(None);

#[cfg(test)]
thread_local! {
    static NON_INTERACTIVE: Cell<bool> = const { Cell::new(false) };
    static QUIET: Cell<bool> = const { Cell::new(false) };
    static OUTPUT_MODE: Cell<OutputMode> = const { Cell::new(OutputMode::Human) };
    static COMMAND: Cell<Option<&'static str>> = const { Cell::new(None) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Human = 0,
    Json = 1,
    ProgressJson = 2,
}

pub fn configure(non_interactive: bool, quiet: bool, output_mode: OutputMode) {
    #[cfg(test)]
    {
        NON_INTERACTIVE.with(|flag| flag.set(non_interactive));
        QUIET.with(|flag| flag.set(quiet));
        OUTPUT_MODE.with(|flag| flag.set(output_mode));
        COMMAND.with(|command| command.set(None));
    }
    #[cfg(not(test))]
    {
        NON_INTERACTIVE.store(non_interactive, Ordering::Relaxed);
        QUIET.store(quiet, Ordering::Relaxed);
        OUTPUT_MODE.store(output_mode as u8, Ordering::Relaxed);
        *COMMAND.lock().unwrap_or_else(|poison| poison.into_inner()) = None;
    }
}

pub fn set_command(command: &'static str) {
    #[cfg(test)]
    {
        COMMAND.with(|current| current.set(Some(command)));
    }
    #[cfg(not(test))]
    {
        *COMMAND.lock().unwrap_or_else(|poison| poison.into_inner()) = Some(command);
    }
}

pub fn command() -> Option<&'static str> {
    #[cfg(test)]
    {
        COMMAND.with(Cell::get)
    }
    #[cfg(not(test))]
    {
        *COMMAND.lock().unwrap_or_else(|poison| poison.into_inner())
    }
}

pub fn is_non_interactive() -> bool {
    #[cfg(test)]
    {
        NON_INTERACTIVE.with(Cell::get)
    }
    #[cfg(not(test))]
    {
        NON_INTERACTIVE.load(Ordering::Relaxed)
    }
}

pub fn is_quiet() -> bool {
    #[cfg(test)]
    {
        QUIET.with(Cell::get)
    }
    #[cfg(not(test))]
    {
        QUIET.load(Ordering::Relaxed)
    }
}

pub fn output_mode() -> OutputMode {
    #[cfg(test)]
    {
        OUTPUT_MODE.with(Cell::get)
    }
    #[cfg(not(test))]
    {
        match OUTPUT_MODE.load(Ordering::Relaxed) {
            1 => OutputMode::Json,
            2 => OutputMode::ProgressJson,
            _ => OutputMode::Human,
        }
    }
}

pub fn is_json() -> bool {
    output_mode() == OutputMode::Json
}

pub fn is_progress_json() -> bool {
    output_mode() == OutputMode::ProgressJson
}

pub fn is_machine_mode() -> bool {
    !matches!(output_mode(), OutputMode::Human)
}

/// Resolve the user's home directory, with a debug-only override for
/// hermetic integration tests that exercise live tool state.
pub fn user_home() -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(path) = crate::auth::test_overrides::string("AISW_TEST_USER_HOME") {
        return Some(PathBuf::from(path));
    }

    dirs::home_dir()
}

#[cfg(test)]
mod tests {
    use super::user_home;
    use crate::auth::test_overrides::EnvVarGuard;
    use tempfile::tempdir;

    #[test]
    fn user_home_honors_the_hermetic_test_override() {
        let _spawn_lock = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let temp = tempdir().unwrap();
        let _home = EnvVarGuard::set("AISW_TEST_USER_HOME", temp.path());

        assert_eq!(user_home().as_deref(), Some(temp.path()));
    }
}
