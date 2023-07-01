#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "Attempts to register some valid domains and makes sure invalid domains cannot be registered."
        exit
fi

set -euox pipefail

source "./test/lib.include"
RAND_DOMAIN=$(random_string)
create_project


run_test spacetime identity new --no-email
IDENT=$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')
run_test spacetime dns register-tld "$RAND_DOMAIN"
create_project
run_test spacetime publish -s -d "$RAND_DOMAIN" --project-path "$PROJECT_PATH" --clear-database
run_test spacetime publish -s -d "$RAND_DOMAIN/test" --project-path "$PROJECT_PATH" --clear-database
run_test spacetime publish -s -d "$RAND_DOMAIN/test/test2" --project-path "$PROJECT_PATH" --clear-database

run_fail_test spacetime publish -s -d "$RAND_DOMAIN//test" --project-path "$PROJECT_PATH" --clear-database
run_fail_test spacetime publish -s -d "$RAND_DOMAIN/test/" --project-path "$PROJECT_PATH" --clear-database
run_fail_test spacetime publish -s -d "$RAND_DOMAIN/test//test2" --project-path "$PROJECT_PATH" --clear-database
