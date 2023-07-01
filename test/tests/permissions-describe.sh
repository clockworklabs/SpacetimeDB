#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test makes sure that anyone can describe any database."
        exit
fi

set -euox pipefail

source "./test/lib.include"

run_test spacetime identity new --no-email
create_project
run_test spacetime publish -s -d --project-path "$PROJECT_PATH" --clear-database
sleep 2
DATABASE="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

reset_config
run_test spacetime identity new --no-email

# It is expected that you should be able to describe any database even if you
# do not own it.
if ! run_test spacetime describe "$DATABASE" ; then exit 1; fi
