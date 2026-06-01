# 0.4.0

- Inherited file descriptors >= 3 are closed before spawning the child preventing accidentally holding onto FDs 

# 0.3.0

- Added `libcontainer` feature with `Crostini` executor
- Added signal forwarding and zombie reaping tests (`tests/sigterm.rs`)

# 0.2.0

- Initial library release
