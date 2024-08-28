#!/bin/sh -eu
: $CL_HOME

cd $CL_HOME/SpacetimeDB/crates/client-api-messages
cargo run --example get_ws_schema > $CL_HOME/schema.json

cd $CL_HOME/SpacetimeDB/crates/cli
cargo run -- generate -l csharp -n SpacetimeDB.ClientApi \
  --json-module $CL_HOME/schema.json \
  -o $CL_HOME/spacetimedb-csharp-sdk/src/SpacetimeDB/ClientApi

cd $CL_HOME/spacetimedb-csharp-sdk/src/SpacetimeDB/ClientApi
rm -rf _Globals

rm -f $CL_HOME/schema.json
