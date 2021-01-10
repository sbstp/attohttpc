#!/bin/bash
set -Eeuxo pipefail

unset http_proxy https_proxy no_proxy
unset HTTP_PROXY HTTPS_PROXY NO_PROXY

if [[ "${CI:-}" == "true" ]] ; then
    mkdir -p .cargo
    echo "[term]" >> .cargo/config.toml
    echo "color = 'always'" >> .cargo/config.toml
fi

cargo test
cargo test --all-features
cargo test --no-default-features
cargo test --no-default-features --features charsets
cargo test --no-default-features --features compress
cargo test --no-default-features --features form
cargo test --no-default-features --features multipart-form
cargo test --no-default-features --features json
cargo test --no-default-features --features tls
cargo test --no-default-features --features tls-rustls
