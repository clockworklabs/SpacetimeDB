#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "NO DESCRIPTION FOR THIS TEST!"
        exit
fi

set -euox pipefail

source "./.test/lib.include"

run_test cargo run server "https://spacetimedb.com/spacetimedb"
[ "$(grep Host "$TEST_OUT")" == "Host: spacetimedb.com/spacetimedb" ]
[ "$(grep Protocol "$TEST_OUT")" == "Protocol: https" ]
[ "$(grep host $HOME/.spacetime/config.toml)" == "host = 'spacetimedb.com/spacetimedb'" ]
[ "$(grep protocol $HOME/.spacetime/config.toml)" == "protocol = 'https'" ]

run_test cargo run server "http://127.0.0.1:3000/spacetimedb"
[ "$(grep Host "$TEST_OUT")" == "Host: 127.0.0.1:3000/spacetimedb" ]
[ "$(grep Protocol "$TEST_OUT")" == "Protocol: http" ]
[ "$(grep host $HOME/.spacetime/config.toml)" == "host = '127.0.0.1:3000/spacetimedb'" ]
[ "$(grep protocol $HOME/.spacetime/config.toml)" == "protocol = 'http'" ]

run_test cargo run server "http://127.0.0.1"
[ "$(grep Host "$TEST_OUT")" == "Host: 127.0.0.1" ]
[ "$(grep Protocol "$TEST_OUT")" == "Protocol: http" ]
[ "$(grep host $HOME/.spacetime/config.toml)" == "host = '127.0.0.1'" ]
[ "$(grep protocol $HOME/.spacetime/config.toml)" == "protocol = 'http'" ]
