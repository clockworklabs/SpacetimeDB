#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
        echo "Ensures cancelling a reducer works"
        exit
fi

set -euox pipefail

source "./test/lib.include"

cat > "${PROJECT_PATH}/src/lib.rs" << EOF
use spacetimedb::{println, spacetimedb, ScheduleToken};

#[spacetimedb(init)]
fn init() {
    let token = spacetimedb::schedule!("100ms", reducer(1));
    token.cancel();
    let token = spacetimedb::schedule!("1000ms", reducer(2));
    spacetimedb::schedule!("500ms", do_cancel(token));
}

#[spacetimedb(reducer)]
fn do_cancel(token: ScheduleToken<reducer>) {
    token.cancel()
}

#[spacetimedb(reducer)]
fn reducer(num: i32) {
    println!("the reducer ran: {}", num)
}
EOF

run_test cargo run publish --skip_clippy --project-path "$PROJECT_PATH" --clear-database
[ "1" == "$(grep -c "reated new database" "$TEST_OUT")" ]
ADDRESS="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"
sleep 2

run_test cargo run logs "$ADDRESS"
! grep -c "the reducer ran" "$TEST_OUT"
