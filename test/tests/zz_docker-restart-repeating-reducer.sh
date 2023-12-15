#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This tests to see if a SpacetimeDB module's repeating reducers are rescheduled after a restart"
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

#[spacetimedb(reducer)]
pub fn dummy() {}
EOF

run_test cargo run publish --skip_clippy --project-path "$PROJECT_PATH" --clear-database
[ "1" == "$(grep -c "reated new database" "$TEST_OUT")" ]
IDENT="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

restart_docker
run_test cargo run call "$IDENT" dummy
sleep 4

run_test cargo run logs "$IDENT"
LINES="$(grep -c "Invoked" "$TEST_OUT")"

sleep 4
run_test cargo run logs "$IDENT"
LINES_NEW="$(grep -c "Invoked" "$TEST_OUT")"
((LINES < LINES_NEW))
