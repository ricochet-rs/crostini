use nix::{
    errno::Errno,
    sys::{
        signal::{SigSet, SigmaskHow, Signal, kill, sigprocmask},
        signalfd::{SfdFlags, SignalFd},
        wait::{WaitPidFlag, WaitStatus, waitpid},
    },
    unistd::Pid,
};
use std::{ffi::OsStr, mem, process::Command};

pub fn run<S: AsRef<OsStr>>(argv: &[S]) -> i32 {
    let mut mask = SigSet::empty();
    mask.add(Signal::SIGTERM);
    mask.add(Signal::SIGINT);
    mask.add(Signal::SIGCHLD);
    sigprocmask(SigmaskHow::SIG_BLOCK, Some(&mask), None).expect("sigprocmask failed");

    let sfd = SignalFd::with_flags(&mask, SfdFlags::SFD_CLOEXEC).expect("signalfd failed");

    let child = Command::new(&argv[0])
        .args(&argv[1..])
        .spawn()
        .unwrap_or_else(|e| {
            eprintln!("crostini: failed to spawn {:?}: {}", argv[0].as_ref(), e);
            std::process::exit(127);
        });

    let child_pid = Pid::from_raw(child.id() as i32);

    let exit_code = 'outer: loop {
        let info = match sfd.read_signal() {
            Ok(Some(i)) => i,
            Ok(None) => continue,
            Err(e) => {
                eprintln!("crostini: signalfd read error: {}", e);
                std::process::exit(1);
            }
        };

        match Signal::try_from(info.ssi_signo as i32).unwrap() {
            Signal::SIGTERM | Signal::SIGINT => {
                let sig = Signal::try_from(info.ssi_signo as i32).unwrap();
                let _ = kill(child_pid, sig);
            }
            Signal::SIGCHLD => loop {
                match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
                    Ok(WaitStatus::Exited(pid, code)) if pid == child_pid => break 'outer code,
                    Ok(WaitStatus::Signaled(pid, sig, _)) if pid == child_pid => {
                        break 'outer 128 + sig as i32;
                    }
                    Ok(WaitStatus::StillAlive) => break,
                    Err(Errno::EINTR) => continue,
                    Err(Errno::ECHILD) => break,
                    Err(_) | Ok(_) => break,
                }
            },
            _ => {}
        }
    };

    loop {
        match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) | Err(_) => break,
            Ok(_) => continue,
        }
    }

    mem::forget(child);
    exit_code
}
