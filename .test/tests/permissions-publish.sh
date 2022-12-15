#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
        echo "This test checks to make sure that you cannot publish to an address that you do not own."
        exit
fi

set -euox pipefail

source "./.test/lib.include"

create_project
run_test cargo run identity new
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
run_test cargo run identity set-default "$IDENT"
spacetime_publish --project-path="$PROJECT_PATH"
ADDRESS="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

reset_config
if spacetime_publish "$ADDRESS" --project-path="$PROJECT_PATH" ; then exit 1; fi
