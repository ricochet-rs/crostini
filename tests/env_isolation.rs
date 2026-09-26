#![cfg(feature = "libcontainer")]

use anyhow::{Result, bail};
use libcontainer::{
    container::{Container, builder::ContainerBuilder},
    oci_spec::runtime::{MountBuilder, Spec},
    syscall::syscall::SyscallType,
};
use nix::{
    sys::wait::{WaitStatus, waitpid},
    unistd::{Pid, getegid, geteuid},
};
use serial_test::serial;
use std::{fs::create_dir_all, path::Path};
use tempfile::{TempDir, tempdir};

const SLOTS: usize = 8;

struct Slot {
    number: usize,
    init: Pid,
    container: Container,
    _root: TempDir,
}

// `setup_envs` runs while a container is created and `exec` runs only once it is started, so
// creating every slot before starting any leaves all eight inits holding an environment at once.
// A shared `CONTAINER_ENVS` would hand a slot its sibling's `CROSTINI_SLOT`, and the workload
// exits non-zero when the value is not its own.
#[test]
#[serial]
fn concurrent_containers_each_see_their_own_environment() -> Result<()> {
    let mut slots = Vec::with_capacity(SLOTS);
    for number in 0..SLOTS {
        slots.push(create_slot(number)?);
    }

    let mut failures = Vec::new();
    for slot in &mut slots {
        if let Err(err) = slot.container.start() {
            failures.push(format!("slot {} could not start: {err}", slot.number));
        }
    }

    for slot in &slots {
        match waitpid(slot.init, None) {
            Ok(WaitStatus::Exited(_, 0)) => {}
            Ok(WaitStatus::Exited(_, code)) => failures.push(format!(
                "slot {} did not see CROSTINI_SLOT={} (exit {code})",
                slot.number, slot.number
            )),
            Ok(status) => failures.push(format!("slot {} ended as {status:?}", slot.number)),
            Err(err) => failures.push(format!(
                "slot {} could not be waited on: {err}",
                slot.number
            )),
        }
    }

    for slot in &mut slots {
        let _ = slot.container.delete(true);
    }

    if !failures.is_empty() {
        bail!("{}", failures.join("; "));
    }
    Ok(())
}

/// Create one container whose workload asserts that `CROSTINI_SLOT` holds its own number.
fn create_slot(number: usize) -> Result<Slot> {
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
            format!("CROSTINI_SLOT={number}"),
        ]));
        process.set_args(Some(vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("test \"$CROSTINI_SLOT\" = \"{number}\""),
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

    let container = ContainerBuilder::new(
        format!("crostini-env-isolation-{number}"),
        SyscallType::Linux,
    )
    .with_executor(crostini::Crostini)
    .with_root_path(&state)?
    .as_init(&bundle)
    .with_systemd(false)
    .build()?;
    let init = container
        .pid()
        .ok_or_else(|| anyhow::anyhow!("container has no init pid"))?;

    Ok(Slot {
        number,
        init: Pid::from_raw(init.as_raw()),
        container,
        _root: root,
    })
}
