#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "Tests the functionality of the setname command."
        exit
fi

set -euox pipefail

source "./.test/lib.include"

reset_config
run_test cargo run identity init-default
create_project
spacetime_publish --project-path "$PROJECT_PATH"
ADDRESS="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

RAND_NAME="$(random_string)"
run_test cargo run setname "$RAND_NAME" "$ADDRESS"
run_test cargo run dns "$RAND_NAME"
[ "$(cat "$TEST_OUT" | tail -n 1)" == "$ADDRESS" ]

run_test cargo run reversedns "$ADDRESS"
[ "$(cat "$TEST_OUT" | tail -n 1)" == "$RAND_NAME" ]
