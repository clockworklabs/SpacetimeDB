#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "NO DESCRIPTION FOR THIS TEST!"
        exit
fi

set -euox pipefail

source "./test/lib.include"

run_test cargo run server "https://spacetimedb.com/spacetimedb"
[ "$(grep Host "$TEST_OUT")" == "Host: spacetimedb.com/spacetimedb" ]
[ "$(grep Protocol "$TEST_OUT")" == "Protocol: https" ]
[ "$(grep host $SPACETIME_CONFIG_FILE)" == "host = 'spacetimedb.com/spacetimedb'" ]
[ "$(grep protocol $SPACETIME_CONFIG_FILE)" == "protocol = 'https'" ]

run_test cargo run server "http://127.0.0.1:3000/spacetimedb"
[ "$(grep Host "$TEST_OUT")" == "Host: 127.0.0.1:3000/spacetimedb" ]
[ "$(grep Protocol "$TEST_OUT")" == "Protocol: http" ]
[ "$(grep host $SPACETIME_CONFIG_FILE)" == "host = '127.0.0.1:3000/spacetimedb'" ]
[ "$(grep protocol $SPACETIME_CONFIG_FILE)" == "protocol = 'http'" ]

run_test cargo run server "http://127.0.0.1"
[ "$(grep Host "$TEST_OUT")" == "Host: 127.0.0.1" ]
[ "$(grep Protocol "$TEST_OUT")" == "Protocol: http" ]
[ "$(grep host $SPACETIME_CONFIG_FILE)" == "host = '127.0.0.1'" ]
[ "$(grep protocol $SPACETIME_CONFIG_FILE)" == "protocol = 'http'" ]
