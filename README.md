# crostini 🥖

A smol Rust library that provides a minimal PID 1 init process for OCI containers.

`crostini` is **not** a binary. Instead, it is intended to be compiled directly into a Rust container runtime unlike [`tini`](https://github.com/krallin/tini) or [`catatonit`](https://github.com/openSUSE/catatonit) which are used as executables.

## Purpose

Specifically, `crostini` is designed for use with [libcontainer](https://github.com/youki-dev/youki) from the [youki](https://github.com/youki-dev/youki) project, which powers [ricochet's](https://ricochet.rs) rootless container runtime for spawning R, Julia, and Python applications in safe execution environments.

When a rootless container is run as PID 1 inside of the Linux PID namespace, signals such as `SIGTERM` are ignored if there is no explicit handler. This causes, for example, an R process to ignore a graceful shutdown request.

`crostini` solves this by acting as the [`Executor`](https://docs.rs/libcontainer/latest/libcontainer/workload/trait.Executor.html) of the libcontainer process.


## Usage

Enable the `libcontainer` feature to get the `Crostini` executor, which implements `libcontainer::workload::Executor` and can be passed directly to `ContainerBuilder`.

```toml
[dependencies]
crostini = { version = "0.3", features = ["libcontainer"] }
```

```rust
use libcontainer::container::builder::ContainerBuilder;
use libcontainer::syscall::syscall::SyscallType;

let container = ContainerBuilder::new("my-container".to_string(), SyscallType::Linux)
    .with_root_path("/run/containers")?
    .with_executor(crostini::Crostini)
    .as_init("/path/to/bundle")
    .with_systemd(false)
    .build()?;
```

### Standalone

`crostini::run` can also be called directly if you are managing the process lifecycle yourself.

```toml
[dependencies]
crostini = "0.3"
```

```rust
fn main() {
    let argv = vec!["/opt/R/4.5.3/bin/R", "-f", "/app/plumber.R"];
    std::process::exit(crostini::run(&argv));
}
```

`run` accepts any `&[S]` where `S: AsRef<OsStr>` and returns the child's exit code as an `i32`. If the child exits normally, the exit code is returned directly. If the child is killed by a signal, the return value is `128 + signal_number`, following the standard Unix convention.

## Safety

`crostini` is `#![forbid(unsafe_code)]`. All POSIX interactions go through the [`nix`](https://crates.io/crates/nix) crate, which provides safe Rust wrappers over the underlying syscalls.

## Dependencies

`crostini` depends only on [`nix`](https://crates.io/crates/nix) for safe POSIX bindings. The `libcontainer` feature adds an optional dependency on [`libcontainer`](https://crates.io/crates/libcontainer).
