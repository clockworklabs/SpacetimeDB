#!/usr/bin/env bash

script_dir="$(readlink -f "$(dirname "$0")")"
stdb_root="$(realpath "$script_dir/../")"

set -euox pipefail

cd "$stdb_root"
cargo check --all --tests --benches
cargo fmt --all -- --check
cargo clippy --all --tests --benches -- -D warnings

