default:
    just --list

test:
    cargo test -F libcontainer

fmt:
    cargo fmt

push:
    git add . && git commit -m "update" && git push
