#![cfg(feature = "libcontainer")]

use std::fs::create_dir_all;
use std::hash::{DefaultHasher, Hash, Hasher};
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
    eprintln!("hello");
    let root = tempdir()?;
    let bundle = root.path().join("bundle");
    let state = root.path().join("state");
    create_dir_all(&state)?;
    eprintln!("hello");
    let id = format!("crostini-test-{:x}", hash(root.path()));
    eprintln!("hello");
    prepare_bundle(&bundle)?;
    eprintln!("hello");
    let container = ContainerBuilder::new(id, SyscallType::Linux)
        .with_executor(crostini::Crostini)
        .with_root_path(&state)?
        .as_init(&bundle)
        .with_systemd(use_systemd())
        .build()?;

    eprintln!("hello");
    let init_pid = Pid::from_raw(container.pid().unwrap().as_raw());

    eprintln!("hello");
    let container = scopeguard::guard(container, |mut c| {
        let _ = c.delete(true);
    });

    eprintln!("hello");
    // Give crostini time to spawn its child before sending SIGTERM.
    std::thread::sleep(std::time::Duration::from_secs(1));
    eprintln!("hello");
    kill(init_pid, Signal::SIGTERM)?;
    eprintln!("hello");
    let status = waitpid(init_pid, Some(WaitPidFlag::empty()))?;
    eprintln!("hello");
    match status {
        WaitStatus::Exited(_, code) => assert_eq!(code, 143),
        other => panic!("unexpected wait status: {other:?}"),
    }

    Ok(())
}
