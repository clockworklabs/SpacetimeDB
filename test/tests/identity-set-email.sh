#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
    echo 'This test is designed to test the identity set-email functionality'
    exit
fi

set -euox pipefail

source "./test/lib.include"

# Create a new identity
run_test spacetime identity new --no-email
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
EMAIL="$(random_string)@clockworklabs.io"
TOKEN="$(spacetime identity token "$IDENT")"

# Reset our config so we lose this identity
reset_config

# Import this identity, and set it as the default identity
run_test spacetime identity import "$IDENT" "$TOKEN"
run_test spacetime identity set-default "$IDENT"

# Configure our email
run_test spacetime identity set-email "$IDENT" "$EMAIL"
[ "$IDENT" == "$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')" ]
[ "$EMAIL" == "$(grep EMAIL "$TEST_OUT" | awk '{print $2}')" ]

# Reset config again
reset_config

# Find our identity by its email
run_test spacetime identity find "$EMAIL"
[ "$IDENT" == "$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')" ]
[ "$EMAIL" == "$(grep EMAIL "$TEST_OUT" | awk '{print $2}')" ]
