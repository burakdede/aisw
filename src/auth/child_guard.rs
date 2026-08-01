//! RAII wrapper that guarantees a spawned child process is reaped.
//!
//! The OAuth capture loops poll a live `claude auth login` / `codex login`
//! child while watching for credentials to appear. Those loops contain
//! fallible steps (reading a credential file, querying the keychain, polling
//! the child), and an early `?` return there used to leave the interactive
//! child running with the terminal still attached to it.

use std::process::Child;

pub(crate) struct ChildGuard {
    child: Option<Child>,
}

impl ChildGuard {
    pub(crate) fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    pub(crate) fn as_mut(&mut self) -> &mut Child {
        self.child
            .as_mut()
            .expect("child is only taken in Drop, which consumes the guard")
    }

    /// Kill and reap the child now. Idempotent, and safe to call on a child
    /// that already exited.
    pub(crate) fn terminate(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.terminate();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    fn spawn_sleeper() -> Child {
        Command::new("/bin/sh")
            .args(["-c", "sleep 30"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("sleeper should spawn")
    }

    fn is_running(pid: u32) -> bool {
        // Signal 0 probes for existence without delivering a signal. A reaped
        // child is gone entirely; a zombie would still report as present, so
        // this also proves the guard waits rather than only killing.
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }

    #[test]
    fn dropping_the_guard_kills_and_reaps_the_child() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let child = spawn_sleeper();
        let pid = child.id();

        drop(ChildGuard::new(child));

        assert!(
            !is_running(pid),
            "guard must kill and reap the child on drop"
        );
    }

    /// This is the case that regressed: an error inside the polling loop.
    #[test]
    fn an_early_error_return_does_not_orphan_the_child() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let child = spawn_sleeper();
        let pid = child.id();

        let result: anyhow::Result<()> = (|| {
            let _guard = ChildGuard::new(child);
            anyhow::bail!("simulated failure while polling for credentials")
        })();

        assert!(result.is_err());
        assert!(!is_running(pid), "child must not outlive a failed capture");
    }

    #[test]
    fn terminate_is_idempotent() {
        let _g = crate::SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let child = spawn_sleeper();
        let pid = child.id();

        let mut guard = ChildGuard::new(child);
        guard.terminate();
        guard.terminate();
        drop(guard);

        assert!(!is_running(pid));
    }
}
