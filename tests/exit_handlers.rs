#![cfg(feature = "libcontainer")]

use anyhow::Result;
use crostini::Crostini;
use libcontainer::{oci_spec::runtime::Spec, workload::Executor};
use nix::{
    libc,
    sys::wait::{WaitStatus, waitpid},
    unistd::{ForkResult, fork},
};

const HANDLER_RAN: i32 = 42;

extern "C" fn exit_from_handler() {
    unsafe { libc::_exit(HANDLER_RAN) }
}

// An inherited handler can wait forever on a lock held by a thread the forked init does not have.
#[test]
fn init_exits_without_running_inherited_exit_handlers() -> Result<()> {
    let mut spec = Spec::default();
    if let Some(process) = spec.process_mut() {
        process.set_args(Some(["/bin/sh", "-c", "exit 7"].map(String::from).to_vec()));
    }

    match unsafe { fork() }? {
        ForkResult::Child => {
            unsafe { libc::atexit(exit_from_handler) };
            let _ = Crostini.exec(&spec);
            unsafe { libc::_exit(1) }
        }
        ForkResult::Parent { child } => {
            assert_eq!(waitpid(child, None)?, WaitStatus::Exited(child, 7));
            Ok(())
        }
    }
}
