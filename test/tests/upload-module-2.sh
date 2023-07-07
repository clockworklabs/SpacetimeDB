#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
        echo "This test deploys a module with a repeating reducer and checks the logs to make sure its running."
        exit
fi

set -euox pipefail

source "./test/lib.include"

cat > "${PROJECT_PATH}/src/lib.rs" << EOF
use spacetimedb::{println, spacetimedb, Timestamp};

#[spacetimedb(init)]
fn init() {
    spacetimedb::schedule!("100ms", my_repeating_reducer(Timestamp::now()));
}

#[spacetimedb(reducer, repeat = 100ms)]
pub fn my_repeating_reducer(prev: Timestamp) {
    println!("Invoked: ts={:?}, delta={:?}", Timestamp::now(), prev.elapsed());
}
EOF

echo "CONFIG: $SPACETIME_CONFIG_FILE"
cat "$SPACETIME_CONFIG_FILE"
run_test "$SPACETIME" publish -s -d --project-path "$PROJECT_PATH" --clear-database
[ "1" == "$(grep -c "reated new database" "$TEST_OUT")" ]
ADDRESS="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"
sleep 2

echo "CONFIG: $SPACETIME_CONFIG_FILE"
cat "$SPACETIME_CONFIG_FILE"
run_test "$SPACETIME" logs "$ADDRESS"
LINES="$(grep -c "Invoked" "$TEST_OUT")"

sleep 4
echo "CONFIG: $SPACETIME_CONFIG_FILE"
cat "$SPACETIME_CONFIG_FILE"
run_test "$SPACETIME" logs "$ADDRESS"
LINES_NEW="$(grep -c "Invoked" "$TEST_OUT")"
((LINES < LINES_NEW))
