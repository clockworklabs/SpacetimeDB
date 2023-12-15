#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "Attempts to register some valid domains and makes sure invalid domains cannot be registered."
        exit
fi

set -euox pipefail

source "./test/lib.include"
RAND_DOMAIN=$(random_string)


run_test cargo run identity new --no-email
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
run_test cargo run dns register-tld "$RAND_DOMAIN"
clear_project
reset_project
run_test cargo run publish --skip_clippy "$RAND_DOMAIN" --project-path "$PROJECT_PATH" --clear-database
run_test cargo run publish --skip_clippy "$RAND_DOMAIN/test" --project-path "$PROJECT_PATH" --clear-database
run_test cargo run publish --skip_clippy "$RAND_DOMAIN/test/test2" --project-path "$PROJECT_PATH" --clear-database

run_fail_test cargo run publish --skip_clippy "$RAND_DOMAIN//test" --project-path "$PROJECT_PATH" --clear-database
run_fail_test cargo run publish --skip_clippy "$RAND_DOMAIN/test/" --project-path "$PROJECT_PATH" --clear-database
run_fail_test cargo run publish --skip_clippy "$RAND_DOMAIN/test//test2" --project-path "$PROJECT_PATH" --clear-database
