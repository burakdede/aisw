#[cfg(unix)]
pub struct TerminalGuard {
    original: Option<libc::termios>,
    restore_on_drop: bool,
    restored: std::cell::Cell<bool>,
}

#[cfg(not(unix))]
pub struct TerminalGuard;

#[cfg(unix)]
impl TerminalGuard {
    /// Snapshot terminal state without restoring it on drop.
    ///
    /// Use this for read-only inspection only. A restore is a terminal-setting
    /// operation and can stop a Linux background process group with SIGTTOU.
    pub fn capture() -> Self {
        Self::capture_with_restore(false)
    }

    /// Snapshot terminal state and restore it on explicit restore or drop.
    ///
    /// Use this only around code that may leave the shared terminal modified,
    /// such as an interactive child process.
    pub fn capture_for_restore() -> Self {
        Self::capture_with_restore(true)
    }

    fn capture_with_restore(restore_on_drop: bool) -> Self {
        if unsafe { libc::isatty(libc::STDIN_FILENO) } != 1 {
            return Self {
                original: None,
                restore_on_drop,
                restored: std::cell::Cell::new(true),
            };
        }

        let mut term = std::mem::MaybeUninit::<libc::termios>::uninit();
        let rc = unsafe { libc::tcgetattr(libc::STDIN_FILENO, term.as_mut_ptr()) };
        if rc != 0 {
            return Self {
                original: None,
                restore_on_drop,
                restored: std::cell::Cell::new(true),
            };
        }

        Self {
            original: Some(unsafe { term.assume_init() }),
            restore_on_drop,
            restored: std::cell::Cell::new(false),
        }
    }

    pub fn restore(&self) {
        if self.restored.get() {
            return;
        }

        let Some(original) = self.original.as_ref() else {
            return;
        };
        let _ = unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, original) };
        self.restored.set(true);
    }
}

#[cfg(not(unix))]
impl TerminalGuard {
    pub fn capture() -> Self {
        Self
    }

    pub fn capture_for_restore() -> Self {
        Self
    }

    pub fn restore(&self) {}
}

#[cfg(unix)]
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.restore_on_drop {
            self.restore();
        }
    }
}

#[cfg(not(unix))]
impl Drop for TerminalGuard {
    fn drop(&mut self) {}
}
