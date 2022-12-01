#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test makes sure that we are not able to view the logs of a module that we don't have permission to view."
	exit
fi

set -euox pipefail

source "./.test/lib.include"

run_test cargo run identity new
run_test cargo run init --lang rust "$PROJECT_PATH"
run_test cargo run publish --project-path "$PROJECT_PATH"
sleep 2
DATABASE="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

reset_config
run_test cargo run identity new
run_test cargo run call "$DATABASE" "say_hello"

reset_config
run_test cargo run identity new
run_test cargo run logs "$DATABASE" 10000
if [ "0" != "$(grep -c "World" "$TEST_OUT")" ]; then exit 1; fi
