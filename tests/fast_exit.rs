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
use std::{fs::create_dir_all, io::Write, path::Path, sync::mpsc, thread, time::Duration};
use tempfile::tempdir;

/// Stalls every crostini log line, which the forked init inherits, so the workload exits before signals are masked.
struct SlowStderr;

impl Write for SlowStderr {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        thread::sleep(Duration::from_millis(50));
        std::io::stderr().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        std::io::stderr().flush()
    }
}

#[test]
fn init_exits_with_a_child_that_exits_immediately() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("crostini=info")
        .with_writer(|| SlowStderr)
        .init();

    let mut hung = 0;
    for i in 0..3 {
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
            process.set_args(Some(vec!["/usr/bin/true".to_string()]));
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
        thread::spawn(move || -> Result<()> {
            let mut container =
                ContainerBuilder::new(format!("crostini-fast-exit-{i}"), SyscallType::Linux)
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
            tx.send(status)?;
            Ok(())
        });
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(status) => assert!(matches!(status, WaitStatus::Exited(_, 0)), "{status:?}"),
            Err(_) => hung += 1,
        }
    }

    assert_eq!(hung, 0, "init outlived its child");
    Ok(())
}
