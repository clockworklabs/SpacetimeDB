#!/usr/bin/env bash

set -ueo pipefail

SDK_PATH="$(dirname "$0")/.."
SDK_PATH="$(realpath "$SDK_PATH")"
STDB_PATH="$SDK_PATH/../.."

cd "$STDB_PATH"
cargo build -p spacetimedb-standalone

cargo run -p spacetimedb-client-api-messages --example get_ws_schema |
cargo run -p spacetimedb-cli -- generate -l csharp --namespace SpacetimeDB.ClientApi \
  --module-def \
  -o $SDK_PATH/src/SpacetimeDB/ClientApi/.output

mv $SDK_PATH/src/SpacetimeDB/ClientApi/.output/Types/* $SDK_PATH/src/SpacetimeDB/ClientApi/
rm -rf $SDK_PATH/src/SpacetimeDB/ClientApi/.output
