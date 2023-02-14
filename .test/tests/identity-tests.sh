#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo 'This test is designed to test the "spacetime identity" subcommand.'
        exit
fi

set -euox pipefail

source "./.test/lib.include"

# Create a new identity
run_test cargo run identity new --no-email
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
EMAIL="$(random_string)@clockworklabs.io"
TOKEN=$(grep token "$HOME/.spacetime/config.toml" | awk '{print $3}' | tr -d \')

# Reset our config so we lose this identity
reset_config

# Import this identity, and set it as the default identity
run_test cargo run identity import "$IDENT" "$TOKEN"
run_test cargo run identity set-default --identity "$IDENT"

# Configure our email
run_test cargo run identity set-email --identity "$IDENT" "$EMAIL"
[ "$IDENT" == "$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')" ]
[ "$EMAIL" == "$(grep EMAIL "$TEST_OUT" | awk '{print $2}')" ]

# Reset config again
reset_config

# Find our identity by its email
run_test cargo run identity find "$EMAIL"
[ "$IDENT" == "$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')" ]
[ "$EMAIL" == "$(grep EMAIL "$TEST_OUT" | awk '{print $2}')" ]
