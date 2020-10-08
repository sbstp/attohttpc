#!/bin/sh

cargo clippy --all-features --all-targets -- --deny warnings
