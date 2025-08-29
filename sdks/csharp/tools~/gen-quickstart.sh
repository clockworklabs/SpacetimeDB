#!/usr/bin/env bash

set -ueo pipefail

# Move to the location of this script.
cd "$(dirname "$0")"

# This script is in "sdks/csharp/tools", so we need to go up 3 levels for the root of the repo.
STDB_PATH="${1:-../../../}"
# One level up gets us to the root of the csharp sdk.
SDK_PATH="$(realpath "..")"

cargo build --manifest-path "$STDB_PATH/crates/standalone/Cargo.toml"
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- generate -y -l csharp -o "$SDK_PATH/examples~/quickstart-chat/client/module_bindings" --project-path "$STDB_PATH/modules/quickstart-chat"
