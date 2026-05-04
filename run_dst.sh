#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")"

# madsim-tokio and madsim still use cfg(madsim). SpacetimeDB crates derive
# cfg(simulation) from it in build.rs so source gates can stay simulator-provider
# neutral. Passing only --cfg simulation leaves madsim in std/Tokio mode.
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }--cfg madsim"
exec cargo run -p spacetimedb-dst -- "$@"
