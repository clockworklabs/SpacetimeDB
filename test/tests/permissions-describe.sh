#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test makes sure that anyone can describe any database."
        exit
fi

set -euox pipefail

source "./test/lib.include"

run_test cargo run identity new --no-email
run_test cargo run publish --skip_clippy --project-path "$PROJECT_PATH" --clear-database
sleep 2
DATABASE="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

reset_config
run_test cargo run identity new --no-email

# It is expected that you should be able to describe any database even if you
# do not own it.
if ! run_test cargo run describe "$DATABASE" ; then exit 1; fi
