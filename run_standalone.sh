#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")"

export STDB_PATH=`mktemp -d`
echo "DATABASE AT ${STDB_PATH}"

cp crates/standalone/log4rs.yaml $STDB_PATH/log4rs.yaml
sed -i -- "s|/var/log/|${STDB_PATH}|g" $STDB_PATH/log4rs.yaml

#cat $STDB_PATH/log4rs.yaml
export SPACETIMEDB_LOG_CONFIG=$STDB_PATH/log4rs.yaml

cargo run -p spacetimedb-standalone -- start -l 127.0.0.1:3000