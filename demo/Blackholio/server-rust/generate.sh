#!/bin/bash

set -euo pipefail

cd "$(readlink -f "$(dirname "$0")")"

function spacetime() {
  "$WORK"/SpacetimeDBPrivate/public/target/debug/spacetimedb-cli "$@"
}
spacetime generate --out-dir ../client-unity/Assets/Scripts/autogen --lang cs $@
spacetime generate --lang unrealcpp --uproject-dir ../client-unreal --project-path ./ --module-name client_unreal
