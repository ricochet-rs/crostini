use libcontainer::{
    oci_spec::runtime::Spec,
    workload::{Executor, ExecutorError, ExecutorSetEnvsError, ExecutorValidationError},
};
use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

/// The container environment, kept out of `std::env` because this init was forked with `ENV_LOCK` possibly held.
static CONTAINER_ENVS: OnceLock<HashMap<String, String>> = OnceLock::new();

/// A [`libcontainer`] [`Executor`] that runs the container workload under a correct PID 1 init.
///
/// `Crostini` supervises the workload like [`crate::run`], providing signal forwarding and zombie
/// reaping when libcontainer places your process inside a PID namespace. Pass it to
/// [`ContainerBuilder::with_executor`](libcontainer::container::builder::ContainerBuilder::with_executor)
/// when building a container.
///
/// # Example
///
/// ```rust,no_run
/// use libcontainer::container::builder::ContainerBuilder;
/// use libcontainer::syscall::syscall::SyscallType;
///
/// let container = ContainerBuilder::new("my-container".to_string(), SyscallType::Linux)
///     .with_root_path("/run/containers")?
///     .with_executor(crostini::Crostini)
///     .as_init("/path/to/bundle")
///     .with_systemd(false)
///     .build()?;
/// # Ok::<_, Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone)]
pub struct Crostini;

impl Executor for Crostini {
    fn validate(&self, spec: &Spec) -> Result<(), ExecutorValidationError> {
        let has_args = spec
            .process()
            .as_ref()
            .and_then(|p| p.args().as_ref())
            .is_some_and(|a| !a.is_empty());

        if !has_args {
            return Err(ExecutorValidationError::ArgValidationError(
                "no arguments provided to execute".into(),
            ));
        }

        Ok(())
    }

    fn setup_envs(&self, envs: HashMap<String, String>) -> Result<(), ExecutorSetEnvsError> {
        CONTAINER_ENVS
            .set(envs)
            .map_err(|_| ExecutorSetEnvsError::Other("container environment already set".into()))
    }

    fn exec(&self, spec: &Spec) -> Result<(), ExecutorError> {
        let (program, args) = spec
            .process()
            .as_ref()
            .and_then(|p| p.args().as_ref())
            .and_then(|a| a.split_first())
            .ok_or(ExecutorError::InvalidArg)?;

        let envs = CONTAINER_ENVS.get();
        // std forks for a bare name once the environment is replaced, and glibc's fork waits on malloc locks this clone inherited.
        let program = if program.contains('/') {
            PathBuf::from(program)
        } else {
            envs.and_then(|e| e.get("PATH"))
                .map_or("/bin:/usr/bin", String::as_str)
                .split(':')
                .map(|dir| Path::new(if dir.is_empty() { "." } else { dir }).join(program))
                .find(|path| {
                    path.metadata()
                        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
                })
                .ok_or_else(|| ExecutorError::Other(format!("{program} not found in PATH")))?
        };

        let mut command = Command::new(program);
        command
            .args(args)
            .env_clear()
            .envs(envs.into_iter().flatten());

        let exit_code = crate::supervise(command).map_err(|e| ExecutorError::Other(e.to_string()));

        match exit_code {
            Ok(v) => std::process::exit(v),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_envs_leaves_the_process_environment_untouched() {
        let before: HashMap<String, String> = std::env::vars().collect();
        let envs = HashMap::from([("CROSTINI_TEST_VAR".to_string(), "set".to_string())]);

        assert!(Crostini.setup_envs(envs.clone()).is_ok());

        assert_eq!(std::env::vars().collect::<HashMap<_, _>>(), before);
        assert_eq!(CONTAINER_ENVS.get(), Some(&envs));
    }
}
