#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test makes sure that anyone has the permission to call any standard reducer"
        exit
fi

set -euox pipefail

source "./.test/lib.include"

run_test cargo run identity new
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
TOKEN=$(grep token "$HOME/.spacetime/config.toml" | awk '{print $3}' | tr -d \')
create_project
spacetime_publish --project-path "$PROJECT_PATH"
sleep 2
DATABASE="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

reset_config
run_test cargo run identity new
run_test cargo run call "$DATABASE" "say_hello"

reset_config
run_test cargo run identity add "$IDENT" "$TOKEN"
run_test cargo run identity set-default "$IDENT"
run_test cargo run logs "$DATABASE" 10000
if [ "1" != "$(grep -c "World" "$TEST_OUT")" ]; then exit 1; fi
