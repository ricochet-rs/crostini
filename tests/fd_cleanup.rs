#![cfg(feature = "libcontainer")]
use anyhow::Result;
use libcontainer::{
    container::builder::ContainerBuilder,
    oci_spec::runtime::{MountBuilder, Spec},
    syscall::syscall::SyscallType,
    workload::default::DefaultExecutor,
};
use nix::{
    sys::wait::{WaitPidFlag, WaitStatus, waitpid},
    unistd::{Pid, getegid, geteuid},
};
use serial_test::serial;
use std::{
    fs::create_dir_all,
    hash::{DefaultHasher, Hash, Hasher},
    os::unix::io::AsRawFd,
    path::Path,
};
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

#[test]
#[serial]
fn inherited_fds_are_closed_before_child_spawn() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let f1 = std::fs::File::open("/dev/null")?;
    let f2 = std::fs::File::open("/dev/null")?;
    let fd1 = f1.as_raw_fd();
    let fd2 = f2.as_raw_fd();

    use nix::fcntl::{FcntlArg, FdFlag, fcntl};

    fcntl(&f1, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC))?;
    fcntl(&f2, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC))?;

    let root = tempdir()?;
    let bundle = root.path().join("bundle");
    let state = root.path().join("state");
    create_dir_all(&state)?;

    let id = format!("crostini-test-fds-{:x}", hash(root.path()));

    let uid = geteuid().as_raw();
    let gid = getegid().as_raw();

    let rootfs = bundle.join("rootfs");
    for dir in [
        "bin", "lib", "lib64", "usr", "proc", "sys", "dev", "tmp", "run", "opt", "root", "etc",
    ] {
        create_dir_all(rootfs.join(dir))?;
    }

    let mut spec = Spec::rootless(uid, gid);
    if let Some(process) = spec.process_mut() {
        process.set_args(Some(vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("for fd in {fd1} {fd2}; do [ -e /proc/self/fd/$fd ] && exit 1; done; exit 0"),
        ]));
        process.set_cwd("/".into());
    }

    let ro = vec!["bind".to_string(), "ro".to_string()];
    let mut mounts = spec.mounts().clone().unwrap_or_default();
    for path in ["/bin", "/lib", "/usr"] {
        if Path::new(path).exists() {
            mounts.push(
                MountBuilder::default()
                    .destination(path)
                    .typ("bind")
                    .source(path)
                    .options(ro.clone())
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
                .options(ro.clone())
                .build()?,
        );
    }
    spec.set_mounts(Some(mounts));
    spec.save(bundle.join("config.json"))?;

    eprintln!(
        "fd dir snapshot: {:?}",
        std::fs::read_dir("/proc/self/fd")?
            .map(|e| e.unwrap().file_name())
            .collect::<Vec<_>>()
    );
    let container = ContainerBuilder::new(id, SyscallType::Linux)
        // .with_executor(crostini::Crostini)
        .with_executor(DefaultExecutor {})
        .with_root_path(&state)?
        .as_init(&bundle)
        .with_systemd(use_systemd())
        .build()?;

    let init_pid = Pid::from_raw(container.pid().unwrap().as_raw());
    let mut container = scopeguard::guard(container, |mut c| {
        let _ = c.delete(true);
    });

    container.start()?;
    let status = waitpid(init_pid, Some(WaitPidFlag::empty()))?;

    drop(f1);
    drop(f2);

    match status {
        WaitStatus::Exited(_, 0) => Ok(()),
        WaitStatus::Exited(_, _) => {
            anyhow::bail!("fds {fd1} and/or {fd2} were not closed before child spawn")
        }
        other => anyhow::bail!("unexpected wait status: {other:?}"),
    }
}
