#!/usr/bin/env bash

set -ueo pipefail

STDB_PATH="$1"
SDK_PATH="$(dirname "$0")/.."
SDK_PATH="$(realpath "$SDK_PATH")"

cargo run --manifest-path $STDB_PATH/crates/client-api-messages/Cargo.toml --example get_ws_schema |
cargo run --manifest-path $STDB_PATH/crates/cli/Cargo.toml -- generate -l csharp --namespace SpacetimeDB.ClientApi \
  --module-def \
  -o $SDK_PATH/src/SpacetimeDB/ClientApi/.output

mv $SDK_PATH/src/SpacetimeDB/ClientApi/.output/Types/* $SDK_PATH/src/SpacetimeDB/ClientApi/
rm -rf $SDK_PATH/src/SpacetimeDB/ClientApi/.output
