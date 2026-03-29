use std::sync::atomic::{AtomicBool, Ordering};

static NON_INTERACTIVE: AtomicBool = AtomicBool::new(false);
static QUIET: AtomicBool = AtomicBool::new(false);

pub fn configure(non_interactive: bool, quiet: bool) {
    NON_INTERACTIVE.store(non_interactive, Ordering::Relaxed);
    QUIET.store(quiet, Ordering::Relaxed);
}

pub fn is_non_interactive() -> bool {
    NON_INTERACTIVE.load(Ordering::Relaxed)
}

pub fn is_quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}
