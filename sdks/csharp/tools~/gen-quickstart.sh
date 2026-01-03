#!/usr/bin/env bash

set -ueo pipefail

SDK_PATH="$(dirname "$0")/.."
SDK_PATH="$(realpath "$SDK_PATH")"
STDB_PATH="$SDK_PATH/../.."

cd "$STDB_PATH"
cargo build -p spacetimedb-standalone
cargo run -p spacetimedb-cli -- generate -y -l csharp -o "$SDK_PATH/examples~/quickstart-chat/client/module_bindings" --project-path "$SDK_PATH/examples~/quickstart-chat/server"
