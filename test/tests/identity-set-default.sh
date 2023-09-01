#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test makes sure that we are able to set a default identity."
        exit
fi

set -euox pipefail
set -x

source "./test/lib.include"

# Fetch the server's fingerprint.
# The fingerprint is required for `identity list`.
run_test cargo run server fingerprint localhost -f

run_test cargo run identity new --no-email
run_test cargo run identity new --no-email
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
run_test cargo run identity list
[ "0" == "$(grep -F "***" "$TEST_OUT" | grep -c "$IDENT")" ]
run_test cargo run identity set-default "$IDENT"

run_test cargo run identity list
[ "1" == "$(grep -F "***" "$TEST_OUT" | grep -c "$IDENT")" ]
