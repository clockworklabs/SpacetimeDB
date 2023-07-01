#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test makes sure that anyone has the permission to call any standard reducer"
        exit
fi

set -euox pipefail

source "./test/lib.include"

run_test spacetime identity new --no-email
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
TOKEN="$(spacetime identity token "$IDENT")"
create_project
run_test spacetime publish -s -d --project-path "$PROJECT_PATH" --clear-database
sleep 2
DATABASE="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

reset_config
run_test spacetime identity new --no-email
run_test spacetime call "$DATABASE" "say_hello"

reset_config
run_test spacetime identity import "$IDENT" "$TOKEN"
run_test spacetime identity set-default "$IDENT"
run_test spacetime logs "$DATABASE" 10000
if [ "1" != "$(grep -c "World" "$TEST_OUT")" ]; then exit 1; fi
