use libcontainer::{
    oci_spec::runtime::Spec,
    workload::{Executor, ExecutorError, ExecutorValidationError},
};

/// A [`libcontainer`] [`Executor`] that runs the container workload under a correct PID 1 init.
///
/// `Crostini` wraps [`crate::run`] to provide signal forwarding and zombie reaping when
/// libcontainer places your process inside a PID namespace. Pass it to
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
///     .as_init("/path/to/bundle")
///     .with_executor(crostini::Crostini)
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

    fn exec(&self, spec: &Spec) -> Result<(), ExecutorError> {
        let args = spec
            .process()
            .as_ref()
            .and_then(|p| p.args().as_ref())
            .ok_or(ExecutorError::InvalidArg)?;

        let exit_code = crate::run(args.as_slice());
        std::process::exit(exit_code);
    }
}
