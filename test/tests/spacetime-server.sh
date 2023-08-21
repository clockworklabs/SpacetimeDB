#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
    echo "Verify that we can add and list server configurations"
    exit
fi

set -euox pipefail

source "./test/lib.include"

run_test cargo run server add "https://testnet.spacetimedb.com" testnet --no-fingerprint
[ "$(grep Host "$TEST_OUT")" == "Host: testnet.spacetimedb.com" ]
[ "$(grep Protocol "$TEST_OUT")" == "Protocol: https" ]

run_test cargo run server list
[[ "$(grep testnet.spacetimedb.com "$TEST_OUT")" =~ [[:space:]]*testnet\.spacetimedb\.com[[:space:]]+https[[:space:]]+testnet[[:space:]]* ]]
[[ "$(grep 127.0.0.1:3000 "$TEST_OUT")" =~ [[:space:]]*\*\*\*[[:space:]]+127\.0\.0\.1:3000[[:space:]]+http[[:space:]]* ]]
