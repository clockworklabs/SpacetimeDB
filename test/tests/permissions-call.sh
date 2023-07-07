#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test makes sure that anyone has the permission to call any standard reducer"
        exit
fi

set -euox pipefail

source "./test/lib.include"

run_test "$SPACETIME" identity new --no-email
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
TOKEN="$("$SPACETIME" identity token "$IDENT")"
reset_project
run_test "$SPACETIME" publish -s -d --project-path "$PROJECT_PATH" --clear-database
sleep 2
DATABASE="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

reset_config
run_test "$SPACETIME" identity new --no-email
run_test "$SPACETIME" call "$DATABASE" "say_hello"

reset_config
run_test "$SPACETIME" identity import "$IDENT" "$TOKEN"
run_test "$SPACETIME" identity set-default "$IDENT"
run_test "$SPACETIME" logs "$DATABASE" 10000
if [ "1" != "$(grep -c "World" "$TEST_OUT")" ]; then exit 1; fi
