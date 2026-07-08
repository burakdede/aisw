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

#[cfg(test)]
thread_local! {
    static NON_INTERACTIVE: Cell<bool> = const { Cell::new(false) };
    static QUIET: Cell<bool> = const { Cell::new(false) };
    static OUTPUT_MODE: Cell<OutputMode> = const { Cell::new(OutputMode::Human) };
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
    }
    #[cfg(not(test))]
    {
        NON_INTERACTIVE.store(non_interactive, Ordering::Relaxed);
        QUIET.store(quiet, Ordering::Relaxed);
        OUTPUT_MODE.store(output_mode as u8, Ordering::Relaxed);
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
