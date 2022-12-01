#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
        echo "This test checks to make sure that you cannot publish to an address that you do not own."
        exit
fi

set -euox pipefail

source "./.test/lib.include"

run_test cargo run init "$PROJECT_PATH" --lang rust
run_test cargo run identity new
cd "$PROJECT_PATH"
run_test spacetime publish
ADDRESS="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

reset_config
run_test spacetime publish
ADDRESS="$(grep -c "reated new database" "$TEST_OUT")"
[ "$ADDRESS" == 1 ] && exit 1
