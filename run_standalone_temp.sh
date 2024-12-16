#!/bin/bash
# Run a ephemeral database inside a `temp` folder
set -euo pipefail

cd "$(dirname "$0")"

cargo build -p spacetimedb-standalone

STDB_PATH="${STDB_PATH:-$(mktemp -d)}"

function cleanup {
  echo "Removing ${STDB_PATH}"
  rm  -rf "$STDB_PATH"
}

trap cleanup EXIT

echo "DATABASE AT ${STDB_PATH}"

cargo run -p spacetimedb-standalone -- start \
            --data-dir ${STDB_PATH} \
            --jwt-pub-key-path "${STDB_PATH}/id_ecdsa.pub" \
            --jwt-priv-key-path "${STDB_PATH}/id_ecdsa" \
            -l 127.0.0.1:3000 --enable-tracy
