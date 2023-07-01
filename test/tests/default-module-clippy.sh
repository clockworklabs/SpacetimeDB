#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This tests to make sure that the default rust module has no clippy errors or warnings"
        exit
fi

set -euox pipefail

source "./test/lib.include"

reset_project

cd "$PROJECT_PATH"
run_test cargo clippy -- -D warnings
