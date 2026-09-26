#![cfg(feature = "libcontainer")]

use anyhow::Result;
use libcontainer::{
    container::builder::ContainerBuilder,
    oci_spec::runtime::{MountBuilder, Spec},
    syscall::syscall::SyscallType,
};
use nix::{
    sys::wait::{WaitStatus, waitpid},
    unistd::{Pid, getegid, geteuid},
};
use std::{
    fs::create_dir_all,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
use tempfile::tempdir;

// Threads reading `std::env` hold `ENV_LOCK` when libcontainer clones the init process.
#[test]
fn container_init_survives_concurrent_env_readers() -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    let readers: Vec<_> = (0..16)
        .map(|_| {
            let stop = stop.clone();
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    std::hint::black_box(std::env::vars().count());
                }
            })
        })
        .collect();

    let mut hung = 0;
    for i in 0..5 {
        let root = tempdir()?;
        let bundle = root.path().join("bundle");
        let state = root.path().join("state");
        let rootfs = bundle.join("rootfs");
        for dir in ["bin", "lib", "lib64", "usr", "proc", "sys", "dev", "tmp"] {
            create_dir_all(rootfs.join(dir))?;
        }
        create_dir_all(&state)?;

        let mut spec = Spec::rootless(geteuid().as_raw(), getegid().as_raw());
        if let Some(process) = spec.process_mut() {
            process.set_args(Some(vec!["sleep".to_string(), "0.1".to_string()]));
            process.set_cwd("/".into());
        }
        let mut mounts = spec.mounts().clone().unwrap_or_default();
        for path in ["/bin", "/lib", "/lib64", "/usr"] {
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
        spec.set_mounts(Some(mounts));
        spec.save(bundle.join("config.json"))?;

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let run = || -> Result<WaitStatus> {
                let mut container =
                    ContainerBuilder::new(format!("crostini-env-lock-{i}"), SyscallType::Linux)
                        .with_executor(crostini::Crostini)
                        .with_root_path(&state)?
                        .as_init(&bundle)
                        .with_systemd(false)
                        .build()?;
                let init = container
                    .pid()
                    .ok_or(anyhow::anyhow!("container has no init pid"))?;
                container.start()?;
                let status = waitpid(Pid::from_raw(init.as_raw()), None)?;
                let _ = container.delete(true);
                Ok(status)
            };
            let _ = tx.send(run());
        });
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(result) => {
                let status = result?;
                assert!(matches!(status, WaitStatus::Exited(_, 0)), "{status:?}");
            }
            Err(_) => hung += 1,
        }
    }

    stop.store(true, Ordering::Relaxed);
    for reader in readers {
        let _ = reader.join();
    }
    assert_eq!(hung, 0, "container init never reported ready");
    Ok(())
}
