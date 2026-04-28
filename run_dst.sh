#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")"

export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }--cfg madsim"
exec cargo run -p spacetimedb-dst -- "$@"
