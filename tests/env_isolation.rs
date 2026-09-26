#![cfg(feature = "libcontainer")]

use anyhow::{Result, bail};
use libcontainer::{
    container::builder::ContainerBuilder,
    oci_spec::runtime::{MountBuilder, Spec},
    syscall::syscall::SyscallType,
};
use nix::{
    sys::wait::{WaitStatus, waitpid},
    unistd::{Pid, getegid, geteuid},
};
use serial_test::serial;
use std::{fs::create_dir_all, path::Path};
use tempfile::tempdir;

const CONTAINERS: usize = 8;

#[test]
#[serial]
fn each_container_sees_its_own_environment() -> Result<()> {
    let mut failures = Vec::new();
    for id in 0..CONTAINERS {
        match run_container(id) {
            Ok(WaitStatus::Exited(_, 0)) => {}
            Ok(WaitStatus::Exited(_, code)) => failures.push(format!(
                "container {id} did not see CROSTINI_CONTAINER_ID={id} (exit {code})"
            )),
            Ok(status) => failures.push(format!("container {id} ended as {status:?}")),
            Err(err) => failures.push(format!("container {id} could not run: {err}")),
        }
    }

    if !failures.is_empty() {
        bail!("{}", failures.join("; "));
    }
    Ok(())
}

fn run_container(id: usize) -> Result<WaitStatus> {
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
        process.set_env(Some(vec![
            "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
            format!("CROSTINI_CONTAINER_ID={id}"),
        ]));
        process.set_args(Some(vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("test \"$CROSTINI_CONTAINER_ID\" = \"{id}\""),
        ]));
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

    let container =
        ContainerBuilder::new(format!("crostini-env-isolation-{id}"), SyscallType::Linux)
            .with_executor(crostini::Crostini)
            .with_root_path(&state)?
            .as_init(&bundle)
            .with_systemd(false)
            .build()?;
    let init = container
        .pid()
        .ok_or_else(|| anyhow::anyhow!("container has no init pid"))?;
    let init = Pid::from_raw(init.as_raw());
    let mut container = scopeguard::guard(container, |mut c| {
        let _ = c.delete(true);
    });

    container.start()?;
    Ok(waitpid(init, None)?)
}
