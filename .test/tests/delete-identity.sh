#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test checks to see if you're able to delete an identity from your local ~/.spacetime/config.toml file. This test does not require a running remote instance of spacetimedb."
        exit
fi

set -euox pipefail
set -x

source "./.test/lib.include"

run_test cargo run identity new
run_test cargo run identity new
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
run_test cargo run identity new
run_test cargo run identity ls
[ "1" == "$(grep -c "$IDENT" "$TEST_OUT")" ]

run_test cargo run identity delete "$IDENT"
run_test cargo run identity ls
[ "0" == "$(grep -c "$IDENT" "$TEST_OUT")" ]
