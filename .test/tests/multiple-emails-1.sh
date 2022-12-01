#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test checks to see if we're able to register the same email to multiple identities in spacetimedb."
        exit
fi

set -euox pipefail
set -x

source "./.test/lib.include"

EMAIL="$(random_string)@clockworklabs.io"
run_test cargo run identity new
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
run_test cargo run identity set-email "$IDENT" "$EMAIL"

run_test cargo run identity new
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
run_test cargo run identity set-email "$IDENT" "$EMAIL"

reset_config

run_test cargo run identity find "$EMAIL"
[ "2" == "$(grep -c EMAIL "$TEST_OUT")" ]
