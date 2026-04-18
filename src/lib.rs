#![forbid(unsafe_code)]

#[cfg(feature = "libcontainer")]
mod executor;
#[cfg(feature = "libcontainer")]
pub use executor::Crostini;

use nix::{
    errno::Errno,
    sys::{
        signal::{SigSet, SigmaskHow, Signal, killpg, sigprocmask},
        signalfd::{SfdFlags, SignalFd},
        wait::{WaitPidFlag, WaitStatus, waitpid},
    },
    unistd::Pid,
};
use std::{ffi::OsStr, os::unix::process::CommandExt, process::Command};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[tracing::instrument(skip(argv))]
pub fn run<S: AsRef<OsStr>>(argv: &[S]) -> Result<i32> {
    tracing::info!("crostini: starting as PID 1 init");
    tracing::info!(cmd = ?argv[0].as_ref(), "crostini: spawning child");

    #[allow(clippy::zombie_processes)]
    let child = Command::new(&argv[0])
        .args(&argv[1..])
        .process_group(0)
        .spawn()
        .map_err(|e| {
            tracing::error!(cmd = ?argv[0].as_ref(), error = %e, "crostini: failed to spawn child");
            e
        })?;

    let child_pid = Pid::from_raw(child.id() as i32);
    tracing::info!(%child_pid, "crostini: child spawned");

    let mut mask = SigSet::all();
    mask.remove(Signal::SIGKILL);
    mask.remove(Signal::SIGSTOP);
    mask.remove(Signal::SIGSEGV);
    mask.remove(Signal::SIGILL);
    mask.remove(Signal::SIGBUS);
    mask.remove(Signal::SIGABRT);
    mask.remove(Signal::SIGFPE);
    mask.remove(Signal::SIGTRAP);
    mask.remove(Signal::SIGSYS);
    mask.remove(Signal::SIGTTIN);
    mask.remove(Signal::SIGTTOU);

    sigprocmask(SigmaskHow::SIG_SETMASK, Some(&mask), None)?;
    let sfd = SignalFd::with_flags(&mask, SfdFlags::SFD_CLOEXEC)?;

    let exit_code = 'outer: loop {
        let info = match sfd.read_signal() {
            Ok(Some(i)) => i,
            Ok(None) => continue,
            Err(e) => return Err(e.into()),
        };

        let sig = match Signal::try_from(info.ssi_signo as i32) {
            Ok(s) => s,
            Err(_) => continue,
        };

        match sig {
            Signal::SIGCHLD => loop {
                match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
                    Ok(WaitStatus::Exited(pid, code)) if pid == child_pid => {
                        tracing::info!(%child_pid, %code, "crostini: child exited");
                        break 'outer code;
                    }
                    Ok(WaitStatus::Signaled(pid, sig, _)) if pid == child_pid => {
                        tracing::info!(%child_pid, %sig, "crostini: child killed by signal");
                        break 'outer 128 + sig as i32;
                    }
                    Ok(WaitStatus::StillAlive) | Err(Errno::ECHILD) => break,
                    Err(Errno::EINTR) => continue,
                    Ok(_) => continue,
                    Err(_) => break,
                }
            },
            Signal::SIGKILL | Signal::SIGSTOP => {}
            sig => {
                tracing::info!(%sig, %child_pid, "crostini: forwarding signal to child process group");
                let _ = killpg(child_pid, sig);
            }
        }
    };

    let _ = killpg(child_pid, Signal::SIGTERM);
    loop {
        match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) | Err(_) => break,
            Ok(_) => continue,
        }
    }

    Ok(exit_code)
}
