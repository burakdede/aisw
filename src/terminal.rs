#[cfg(unix)]
pub struct TerminalGuard {
    original: Option<libc::termios>,
}

#[cfg(not(unix))]
pub struct TerminalGuard;

#[cfg(unix)]
impl TerminalGuard {
    pub fn capture() -> Self {
        if unsafe { libc::isatty(libc::STDIN_FILENO) } != 1 {
            return Self { original: None };
        }

        let mut term = std::mem::MaybeUninit::<libc::termios>::uninit();
        let rc = unsafe { libc::tcgetattr(libc::STDIN_FILENO, term.as_mut_ptr()) };
        if rc != 0 {
            return Self { original: None };
        }

        Self {
            original: Some(unsafe { term.assume_init() }),
        }
    }

    pub fn restore(&self) {
        let Some(original) = self.original.as_ref() else {
            return;
        };
        let _ = unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, original) };
    }
}

#[cfg(not(unix))]
impl TerminalGuard {
    pub fn capture() -> Self {
        Self
    }

    pub fn restore(&self) {}
}

#[cfg(unix)]
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

#[cfg(not(unix))]
impl Drop for TerminalGuard {
    fn drop(&mut self) {}
}
