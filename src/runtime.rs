#[cfg(not(test))]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
use std::cell::Cell;

#[cfg(not(test))]
static NON_INTERACTIVE: AtomicBool = AtomicBool::new(false);
#[cfg(not(test))]
static QUIET: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
thread_local! {
    static NON_INTERACTIVE: Cell<bool> = const { Cell::new(false) };
    static QUIET: Cell<bool> = const { Cell::new(false) };
}

pub fn configure(non_interactive: bool, quiet: bool) {
    #[cfg(test)]
    {
        NON_INTERACTIVE.with(|flag| flag.set(non_interactive));
        QUIET.with(|flag| flag.set(quiet));
    }
    #[cfg(not(test))]
    {
        NON_INTERACTIVE.store(non_interactive, Ordering::Relaxed);
        QUIET.store(quiet, Ordering::Relaxed);
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
