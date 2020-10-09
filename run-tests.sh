#!/bin/sh

set -exu

unset http_proxy
unset https_proxy
unset no_proxy

unset HTTP_PROXY
unset HTTPS_PROXY
unset NO_PROXY

cargo test --all-features
cargo test --no-default-features
cargo test --no-default-features --features charsets
cargo test --no-default-features --features compress
cargo test --no-default-features --features form
cargo test --no-default-features --features json
cargo test --no-default-features --features tls
cargo test --no-default-features --features tls-rustls
