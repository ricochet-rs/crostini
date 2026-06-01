# Contributing

## Running tests

Integration tests require the `libcontainer` feature and an AppArmor profile to allow unprivileged user namespace creation.

Install the profile:

```bash
sudo cp contrib/apparmor/crostini-tests /etc/apparmor.d/
sudo apparmor_parser -r /etc/apparmor.d/crostini-tests
```

Then run:

```bash
cargo test --features libcontainer
```
