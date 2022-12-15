#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo 'This test is designed to test the "spacetime identity" subcommand.'
        exit
fi

set -euox pipefail

source "./.test/lib.include"

# Create a new identity
run_test cargo run identity new
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
EMAIL="$(random_string)@clockworklabs.io"
TOKEN=$(grep token "$HOME/.spacetime/config.toml" | awk '{print $3}' | tr -d \')

# Reset our config so we lose this identity
reset_config

# Import this identity, and set it as the default identity
run_test cargo run identity add "$IDENT" "$TOKEN"
run_test cargo run identity set-default "$IDENT"

# Configure our email
run_test cargo run identity set-email "$IDENT" "$EMAIL"
[ "$IDENT" == "$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')" ]
[ "$EMAIL" == "$(grep EMAIL "$TEST_OUT" | awk '{print $2}')" ]

# Reset config again
reset_config

# Find our identity by its email
run_test cargo run identity find "$EMAIL"
[ "$IDENT" == "$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')" ]
[ "$EMAIL" == "$(grep EMAIL "$TEST_OUT" | awk '{print $2}')" ]

# Create a new identity and give it the same email, we should now be able to find both identities.
run_test cargo run identity new --email "$EMAIL"
run_test cargo run identity find "$EMAIL"
[ "2" == "$(grep -c EMAIL "$TEST_OUT")" ]
