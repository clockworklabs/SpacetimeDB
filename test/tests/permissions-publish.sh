#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
        echo "This test checks to make sure that you cannot publish to an address that you do not own."
        exit
fi

set -euox pipefail

source "./test/lib.include"

create_project
run_test cargo run identity new --no-email
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
run_test cargo run identity set-default "$IDENT"
run_test cargo run publish -s -d --project-path="$PROJECT_PATH" --clear-database
ADDRESS="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

reset_config
if cargo run publish -s -d "$ADDRESS" --project-path="$PROJECT_PATH" --clear-database ; then exit 1; fi
