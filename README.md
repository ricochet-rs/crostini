# crostini 🥖

A Rust library providing a correct, minimal PID 1 init for OCI containers. Unlike `tini` or `catatonit`, `crostini` is not a standalone binary. It is intended to be compiled directly into a Rust container runtime.

## Purpose

`crostini` is designed for use with [libcontainer](https://github.com/youki-dev/youki) from the [youki](https://github.com/youki-dev/youki) project, which powers [ricochet's](https://ricochet.rs) rootless container runtime for spawning R, Julia, and Python applications in safe execution environments.

When a rootless container is run as PID 1 inside of the Linux PID namespace, the kernel silently ignores signals that have no explicit handler.
This means that there is no graceful shutdown when a `SIGTERM` is sent to a R process, for example.
`crostini` solves this by sitting between the libcontainer runtime and the process. 

`crostini` is as smol as it gets. It handles

- `SIGTERM`,
- `SIGINT`,
- and `SIGCHLD`.

`crostini` has no additional features features beyond correct signal forwarding and zombie reaping.

## Usage

```toml
[dependencies]
crostini = "0.1"
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

`crostini` depends only on [`nix`](https://crates.io/crates/nix) for safe POSIX bindings. The implementation is synchronous and single-file.
