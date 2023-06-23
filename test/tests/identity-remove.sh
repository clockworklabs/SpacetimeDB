#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test checks to see if you're able to delete an identity from your local ~/.spacetime/config.toml file."
        exit
fi

set -euox pipefail
set -x

source "./test/lib.include"

run_test cargo run identity new --no-email
run_test cargo run identity new --no-email
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
run_test cargo run identity list
[ "1" == "$(grep -c "$IDENT" "$TEST_OUT")" ]

run_test cargo run identity remove "$IDENT"
run_test cargo run identity list
[ "0" == "$(grep -c "$IDENT" "$TEST_OUT")" ]

run_fail_test cargo run identity remove "$IDENT"
