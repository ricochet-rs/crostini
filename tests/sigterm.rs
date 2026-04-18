#![cfg(feature = "libcontainer")]

use std::fs::create_dir_all;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{BufRead, BufReader};
use std::os::unix::io::OwnedFd;
use std::path::Path;

use anyhow::Result;
use libcontainer::container::builder::ContainerBuilder;
use libcontainer::oci_spec::runtime::{MountBuilder, Spec};
use libcontainer::syscall::syscall::SyscallType;
use nix::sys::signal::{Signal, kill};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::{Pid, getegid, geteuid};
use serial_test::serial;
use tempfile::tempdir;
use tracing_subscriber::EnvFilter;

fn hash(v: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::default();
    v.hash(&mut hasher);
    hasher.finish()
}

fn use_systemd() -> bool {
    let systemd_running = Path::new("/run/systemd/system").exists()
        && std::fs::read_to_string("/proc/1/comm")
            .map(|c| c.trim() == "systemd")
            .unwrap_or(false);
    systemd_running && std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok()
}

fn prepare_bundle(bundle: &Path) -> Result<()> {
    let rootfs = bundle.join("rootfs");

    for dir in [
        "bin", "lib", "lib64", "usr", "proc", "sys", "dev", "tmp", "run",
    ] {
        create_dir_all(rootfs.join(dir))?;
    }

    let uid = geteuid().as_raw();
    let gid = getegid().as_raw();

    let mut spec = Spec::rootless(uid, gid);

    if let Some(process) = spec.process_mut() {
        process.set_args(Some(vec!["sleep".to_string(), "30".to_string()]));
        process.set_cwd("/".into());
    }

    let mut mounts = spec.mounts().clone().unwrap_or_default();
    for path in ["/bin", "/lib", "/usr"] {
        if Path::new(path).exists() {
            mounts.push(
                MountBuilder::default()
                    .destination(path)
                    .typ("bind")
                    .source(path)
                    .options(vec!["bind".to_string(), "ro".to_string()])
                    .build()?,
            );
        }
    }
    if Path::new("/lib64").exists() {
        mounts.push(
            MountBuilder::default()
                .destination("/lib64")
                .typ("bind")
                .source("/lib64")
                .options(vec!["bind".to_string(), "ro".to_string()])
                .build()?,
        );
    }
    spec.set_mounts(Some(mounts));

    spec.save(bundle.join("config.json"))?;
    Ok(())
}

#[test]
#[serial]
fn sigterm_forwarded_to_child() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let root = tempdir()?;
    let bundle = root.path().join("bundle");
    let state = root.path().join("state");
    create_dir_all(&state)?;

    let id = format!("crostini-test-{:x}", hash(root.path()));
    prepare_bundle(&bundle)?;

    // Pipe container stdout/stderr so eprintln! inside the container process is visible.
    let (stdout_read, stdout_write) = std::os::unix::net::UnixStream::pair()?;
    let (stderr_read, stderr_write) = std::os::unix::net::UnixStream::pair()?;
    let stdout_fd = OwnedFd::from(stdout_write);
    let stderr_fd = OwnedFd::from(stderr_write);

    // Drain container stdout/stderr on background threads.
    std::thread::spawn(move || {
        for line in BufReader::new(stdout_read).lines().map_while(Result::ok) {
            eprintln!("[container stdout] {line}");
        }
    });
    std::thread::spawn(move || {
        for line in BufReader::new(stderr_read).lines().map_while(Result::ok) {
            eprintln!("[container stderr] {line}");
        }
    });

    let container = ContainerBuilder::new(id, SyscallType::Linux)
        .with_executor(crostini::Crostini)
        .with_root_path(&state)?
        .with_stdout(stdout_fd)
        .with_stderr(stderr_fd)
        .as_init(&bundle)
        .with_systemd(use_systemd())
        .build()?;

    let init_pid = Pid::from_raw(container.pid().unwrap().as_raw());
    eprintln!(
        "[test] build() done, init_pid={init_pid}, state={:?}",
        container.state
    );

    let mut container = scopeguard::guard(container, |mut c| {
        let _ = c.delete(true);
    });

    container.start()?;
    eprintln!("[test] start() done, state={:?}", container.state);

    // Give crostini time to spawn its child before sending SIGTERM.
    std::thread::sleep(std::time::Duration::from_secs(1));

    eprintln!("[test] sending SIGTERM to {init_pid}");
    kill(init_pid, Signal::SIGTERM)?;

    eprintln!("[test] waiting on {init_pid}");
    let status = waitpid(init_pid, Some(WaitPidFlag::empty()))?;

    match status {
        WaitStatus::Exited(_, code) => assert_eq!(code, 143),
        other => panic!("unexpected wait status: {other:?}"),
    }

    Ok(())
}
