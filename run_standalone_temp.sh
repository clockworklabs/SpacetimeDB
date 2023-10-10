#!/bin/bash
# Run a ephemeral database inside a `temp` folder
set -euo pipefail

SRC_TREE="$(dirname "$0")"
cd "$SRC_TREE"

declare -r DB_CONFIG_HOME="$HOME/.spacetime/db"
mkdir -p "$DB_CONFIG_HOME"

export SPACETIMEDB_JWT_PRIV_KEY="${SPACETIMEDB_JWT_PRIV_KEY:-$DB_CONFIG_HOME/standalone_id_ecdsa}"
export SPACETIMEDB_JWT_PUB_KEY="${SPACETIMEDB_JWT_PRIV_KEY}.pub"

cargo build -p spacetimedb-standalone

export STDB_PATH="${STDB_PATH:-$(mktemp -d)}"
mkdir -p "$STDB_PATH/logs"

function cleanup {
  echo "Removing ${STDB_PATH}"
  rm  -rf "$STDB_PATH"
}

trap cleanup EXIT

cp crates/standalone/log.conf "$STDB_PATH/log.conf"
# -i differs between GNU and BSD sed, so use a temp file
sed 's/spacetimedb=debug/spacetimedb=trace/g' "$STDB_PATH/log.conf" > "$STDB_PATH/log.conf.tmp" && \
    mv "$STDB_PATH/log.conf.tmp" "$STDB_PATH/log.conf"

export SPACETIMEDB_LOG_CONFIG="$STDB_PATH/log.conf"
export SPACETIMEDB_LOGS_PATH="$STDB_PATH/logs"
export SPACETIMEDB_TRACY=1

echo "DATABASE AT ${STDB_PATH}"
echo "LOGS AT $STDB_PATH/logs"

cargo run -p spacetimedb-standalone -- start -l 127.0.0.1:3000 --enable-tracy
