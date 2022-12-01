#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test makes sure that we are able to set a default identity."
        exit
fi

set -euox pipefail
set -x

source "./.test/lib.include"

run_test cargo run identity new
run_test cargo run identity new
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
run_test cargo run identity ls
[ "0" == "$(grep -F "***" "$TEST_OUT" | grep -c "$IDENT")" ]
run_test cargo run identity set-default "$IDENT"

run_test cargo run identity ls
[ "1" == "$(grep -F "***" "$TEST_OUT" | grep -c "$IDENT")" ]
