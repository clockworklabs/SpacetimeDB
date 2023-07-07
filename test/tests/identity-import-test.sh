#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test tries to import a known good identity to our local ~/.spacetime/config.toml file. This test does not require a remote spacetimedb instance."
        exit
fi

set -euox pipefail

source "./test/lib.include"

run_test "$SPACETIME" identity new --no-email
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')

# TODO: Fix this!
run_test "$SPACETIME" identity list
TOKEN=$(grep token "$HOME/.spacetime/config.toml" | awk '{print $3}' | tr -d \')

reset_config

run_test "$SPACETIME" identity import "$IDENT" "$TOKEN"
run_test "$SPACETIME" identity list
exit 0
[ "$(grep "$IDENT" "$TEST_OUT" | awk '{print $1}')" == '***' ]
