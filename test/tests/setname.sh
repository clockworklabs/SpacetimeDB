#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "Tests the functionality of the setname command."
        exit
fi

set -euox pipefail

source "./test/lib.include"

run_test cargo run identity init-default
run_test cargo run publish --skip_clippy --project-path "$PROJECT_PATH" --clear-database
ADDRESS="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

RAND_NAME="$(random_string)"
run_test cargo run dns register-tld "$RAND_NAME"
run_test cargo run dns set-name "$RAND_NAME" "$ADDRESS"
run_test cargo run dns lookup "$RAND_NAME"
[ "$(cat "$TEST_OUT" | tail -n 1)" == "$ADDRESS" ]

run_test cargo run dns reverse-lookup "$ADDRESS"
[ "$(cat "$TEST_OUT" | tail -n 1)" == "$RAND_NAME" ]
