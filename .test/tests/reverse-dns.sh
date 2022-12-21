#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This tests the functionality of spacetime reverse dns lookups."
        exit
fi

set -euox pipefail

source "./.test/lib.include"

create_project

RAND=$(random_string)
run_test cargo run publish "$RAND" --project-path "$PROJECT_PATH"
ADDRESS="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"
if [ "$ADDRESS" == "" ] ; then
	exit 1
fi

run_test cargo run reversedns "$ADDRESS"
if [ "$RAND" != "$(tail -n 1 $TEST_OUT)" ] ; then
	exit 1
fi
