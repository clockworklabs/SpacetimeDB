#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test checks to see if you're able to register an email with 2 separate identities. This command uses the --email command line flag to associate the email during identity creation."
        exit
fi

set -euox pipefail
set -x

source "./.test/lib.include"

EMAIL="$(random_string)@clockworklabs.io"
run_test cargo run identity new --email "$EMAIL"
run_test cargo run identity new --email "$EMAIL"

reset_config

run_test cargo run identity find "$EMAIL"
[ "2" == "$(grep -c EMAIL "$TEST_OUT")" ]
