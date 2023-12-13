#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test makes sure that anyone has the permission to call any standard reducer"
        exit
fi

set -euox pipefail

source "./test/lib.include"

run_test cargo run identity new --no-email
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
TOKEN="$(cargo run identity token "$IDENT")"
run_test cargo run publish --skip_clippy --project-path "$PROJECT_PATH" --clear-database
sleep 2
DATABASE="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

reset_config
run_test cargo run identity new --no-email
run_test cargo run call "$DATABASE" "say_hello"

reset_config
run_test cargo run identity import "$IDENT" "$TOKEN"
run_test cargo run identity set-default "$IDENT"
run_test cargo run logs "$DATABASE" 10000
if [ "1" != "$(grep -c "World" "$TEST_OUT")" ]; then exit 1; fi
