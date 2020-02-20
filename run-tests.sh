#!/bin/sh

set -exu

cargo test --all-features
cargo test --no-default-features
cargo test --no-default-features --features charsets
cargo test --no-default-features --features compress
cargo test --no-default-features --features form
cargo test --no-default-features --features json
cargo test --no-default-features --features tls
cargo test --no-default-features --features tls-rustls
