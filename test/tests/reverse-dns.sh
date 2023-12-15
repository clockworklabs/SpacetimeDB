#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This tests the functionality of spacetime reverse dns lookups."
        exit
fi

set -euox pipefail

source "./test/lib.include"

RAND=$(random_string)
run_test cargo run dns register-tld "$RAND"
run_test cargo run publish --skip_clippy "$RAND" --project-path "$PROJECT_PATH" --clear-database
ADDRESS="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"
if [ "$ADDRESS" == "" ] ; then
	exit 1
fi

run_test cargo run dns reverse-lookup "$ADDRESS"
if [ "$RAND" != "$(tail -n 1 $TEST_OUT)" ] ; then
	exit 1
fi
