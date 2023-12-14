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

run_test cargo run publish --skip_clippy --project-path "$PROJECT_PATH" --clear-database
[ "1" == "$(grep -c "reated new database" "$TEST_OUT")" ]
ADDRESS="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"
sleep 2

run_test cargo run logs "$ADDRESS"
LINES="$(grep -c "Invoked" "$TEST_OUT")"

sleep 4
run_test cargo run logs "$ADDRESS"
LINES_NEW="$(grep -c "Invoked" "$TEST_OUT")"
((LINES < LINES_NEW))
