#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
    echo "This test is designed to make sure an email can be set while creating a new identity"
    exit
fi

set -euox pipefail

source "./test/lib.include"

# Create a new identity
EMAIL="$(random_string)@clockworklabs.io"
run_test "$SPACETIME" identity new --email "$EMAIL"
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
TOKEN="$("$SPACETIME" identity token "$IDENT")"

# Reset our config so we lose this identity
reset_config

# Import this identity, and set it as the default identity
run_test "$SPACETIME" identity import "$IDENT" "$TOKEN"
run_test "$SPACETIME" identity set-default "$IDENT"

# Configure our email
run_test "$SPACETIME" identity set-email "$IDENT" "$EMAIL"
[ "$IDENT" == "$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')" ]
[ "$EMAIL" == "$(grep EMAIL "$TEST_OUT" | awk '{print $2}')" ]

# Reset config again
reset_config

# Find our identity by its email
run_test "$SPACETIME" identity find "$EMAIL"
[ "$IDENT" == "$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')" ]
[ "$EMAIL" == "$(grep EMAIL "$TEST_OUT" | awk '{print $2}')" ]
