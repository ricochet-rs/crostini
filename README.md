# crostini

A minimal container init process (PID 1) for OCI containers, written in Rust.

Named in the tradition of container inits named after small food items: `tini`, `catatonit`, `dumb-init`.

## Purpose

`crostini` is designed for use with [libcontainer](https://github.com/youki-dev/youki) from the [youki](https://github.com/youki-dev/youki) project, which powers [Ricochet's](https://ricochet.rs) rootless container runtime for spawning R, Julia, and Python applications in safe execution environments.

When a process runs as PID 1 inside a Linux PID namespace, the kernel silently drops signals that have no explicit handler installed. This means `SIGTERM` has no effect on a naive PID 1, preventing graceful shutdown. `crostini` solves this by sitting between the container runtime and the target process: it runs as PID 1, installs proper signal handlers, forwards signals to its child, reaps zombie processes, and exits with the child's exit code.

It handles `SIGTERM`, `SIGINT`, and `SIGCHLD`, with no additional features beyond correct signal forwarding and zombie reaping.

## Usage

```sh
crostini -- <command> [args...]
```

The `--` separator is required. Everything after it becomes the child process argv.

```sh
crostini -- /opt/R/4.4.0/bin/R -s
crostini -- /usr/bin/python3 /app/server.py
crostini -- /bin/sh -c "echo hello"
```

## Exit codes

`crostini` exits with the child's exit code directly if the child exits normally. If the child is killed by a signal, `crostini` exits with `128 + signal_number`, following the standard Unix convention.

## Library usage

`crostini` is also available as a Rust library for runtimes that want to embed init behaviour directly rather than calling an external binary.

```toml
[dependencies]
crostini = "0.1"
```

```rust
fn main() {
    let argv = vec!["/usr/bin/python3".to_string(), "/app/server.py".to_string()];
    std::process::exit(crostini::run(&argv));
}
```

`run` accepts any `&[S]` where `S: AsRef<OsStr>` and returns the child's exit code as an `i32`, following the same `128 + signal_number` convention as the binary.

## Building

`crostini` compiles to a fully static musl binary for `x86_64` and `aarch64`.

```sh
cargo build --release --target x86_64-unknown-linux-musl
cargo build --release --target aarch64-unknown-linux-musl
```

## Dependencies

`crostini` depends only on [`nix`](https://crates.io/crates/nix) for safe POSIX bindings. The implementation is synchronous, single-file, and has zero runtime dependencies in the produced binary.
