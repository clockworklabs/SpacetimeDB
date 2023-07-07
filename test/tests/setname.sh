#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "Tests the functionality of the setname command."
        exit
fi

set -euox pipefail

source "./test/lib.include"

reset_config
run_test "$SPACETIME" identity init-default
reset_project
run_test "$SPACETIME" publish -s -d --project-path "$PROJECT_PATH" --clear-database
ADDRESS="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

RAND_NAME="$(random_string)"
run_test "$SPACETIME" dns register-tld "$RAND_NAME"
run_test "$SPACETIME" dns set-name "$RAND_NAME" "$ADDRESS"
run_test "$SPACETIME" dns lookup "$RAND_NAME"
[ "$(cat "$TEST_OUT" | tail -n 1)" == "$ADDRESS" ]

run_test "$SPACETIME" dns reverse-lookup "$ADDRESS"
[ "$(cat "$TEST_OUT" | tail -n 1)" == "$RAND_NAME" ]
