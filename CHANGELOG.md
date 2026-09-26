# 0.5.1

- The `Crostini` executor passes the container environment to the child instead of writing it into the init process, so container init no longer hangs when another thread of a multi-threaded runtime was reading the environment at fork
- Container init exits when its workload exits before init has finished installing its signal handling, instead of waiting forever for a `SIGCHLD` that was already discarded

# 0.5.0

- Bump `libcontainer` to 0.7.0 which adds rootless cgroup v2 support

# 0.4.0

- Inherited file descriptors >= 3 are closed before spawning the child preventing accidentally holding onto FDs 

# 0.3.0

- Added `libcontainer` feature with `Crostini` executor
- Added signal forwarding and zombie reaping tests (`tests/sigterm.rs`)

# 0.2.0

- Initial library release
