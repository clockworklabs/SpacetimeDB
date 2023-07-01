#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "Tests the functionality of the setname command."
        exit
fi

set -euox pipefail

source "./test/lib.include"

reset_config
run_test spacetime identity init-default
reset_project
run_test spacetime publish -s -d --project-path "$PROJECT_PATH" --clear-database
ADDRESS="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

RAND_NAME="$(random_string)"
run_test spacetime dns register-tld "$RAND_NAME"
run_test spacetime dns set-name "$RAND_NAME" "$ADDRESS"
run_test spacetime dns lookup "$RAND_NAME"
[ "$(cat "$TEST_OUT" | tail -n 1)" == "$ADDRESS" ]

run_test spacetime dns reverse-lookup "$ADDRESS"
[ "$(cat "$TEST_OUT" | tail -n 1)" == "$RAND_NAME" ]
