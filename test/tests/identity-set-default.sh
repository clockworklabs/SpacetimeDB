#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test makes sure that we are able to set a default identity."
        exit
fi

set -euox pipefail
set -x

source "./test/lib.include"

# remove then re-add the server to get a fingerprint for it
run_test cargo run server remove 127.0.0.1:3000
run_test cargo run server add http://127.0.0.1:3000

run_test cargo run identity new --no-email
run_test cargo run identity new --no-email
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
run_test cargo run identity list
[ "0" == "$(grep -F "***" "$TEST_OUT" | grep -c "$IDENT")" ]
run_test cargo run identity set-default "$IDENT"

run_test cargo run identity list
[ "1" == "$(grep -F "***" "$TEST_OUT" | grep -c "$IDENT")" ]
