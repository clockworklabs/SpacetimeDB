#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test just tests to see if the CLI is able to create a local project. This test does not depend on a running spacetimedb instance."
        exit
fi

set -euo pipefail

source "./test/lib.include"

run_fail_test "$SPACETIME" init
run_fail_test "$SPACETIME" init "$PROJECT_PATH"
rm -rf "$PROJECT_PATH"
mkdir -p "$PROJECT_PATH"
run_test "$SPACETIME" init "$PROJECT_PATH" --lang rust
