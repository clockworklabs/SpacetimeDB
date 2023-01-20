#!/bin/bash
set -euo pipefail

DRY_RUN=""

if [ "$#" != "0" ]; then
    if [ "$1" != "--dry-run" ]; then
        echo "$1 is not a valid flag";
        exit 1;
    else
        DRY_RUN=$1
    fi
fi

cd crates/spacetimedb-lib
cargo publish $DRY_RUN
cd ../..

cd crates/spacetimedb-bindings-sys
cargo publish $DRY_RUN
cd ../..

cd crates/spacetimedb-bindings-macro
cargo publish $DRY_RUN
cd ../..

sleep 10

cd crates/spacetimedb-bindings
cargo publish $DRY_RUN
cd ../..

sleep 10

cd crates/spacetimedb-cli
cargo publish $DRY_RUN
cd ../..
