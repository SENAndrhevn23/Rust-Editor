#!/usr/bin/env sh
set -eu
cd "$(dirname "$0")"
cargo run --release
