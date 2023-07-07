#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test makes sure that we are able to set a default identity."
        exit
fi

set -euox pipefail
set -x

source "./test/lib.include"

run_test "$SPACETIME" identity new --no-email
run_test "$SPACETIME" identity new --no-email
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
run_test "$SPACETIME" identity list
[ "0" == "$(grep -F "***" "$TEST_OUT" | grep -c "$IDENT")" ]
run_test "$SPACETIME" identity set-default "$IDENT"

run_test "$SPACETIME" identity list
[ "1" == "$(grep -F "***" "$TEST_OUT" | grep -c "$IDENT")" ]
