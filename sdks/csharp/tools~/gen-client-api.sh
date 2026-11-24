#!/usr/bin/env bash

set -ueo pipefail

SDK_PATH="$(dirname "$0")/.."
SDK_PATH="$(realpath "$SDK_PATH")"
STDB_PATH="$SDK_PATH/../.."

cargo run --manifest-path "$STDB_PATH/crates/client-api-messages/Cargo.toml" --example get_ws_schema_v2 |
  cargo run --manifest-path "$STDB_PATH/crates/codegen/Cargo.toml" --example regen-csharp-clientapi
