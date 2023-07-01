#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This tests the functionality of spacetime reverse dns lookups."
        exit
fi

set -euox pipefail

source "./test/lib.include"

reset_project

RAND=$(random_string)
run_test spacetime dns register-tld "$RAND"
run_test spacetime publish -s -d "$RAND" --project-path "$PROJECT_PATH" --clear-database
ADDRESS="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"
if [ "$ADDRESS" == "" ] ; then
	exit 1
fi

run_test spacetime dns reverse-lookup "$ADDRESS"
if [ "$RAND" != "$(tail -n 1 $TEST_OUT)" ] ; then
	exit 1
fi
