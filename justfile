set shell := ["nu", "-c"]

watch:
    watch . { cargo run } --glob=**/*.rs

run:
    cargo run

test:
    cargo test

watch-tests:
    watch . { cargo test } --glob=**/*.rs
