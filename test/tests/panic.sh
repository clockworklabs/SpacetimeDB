#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "Tests to check if a SpacetimeDB module can handle a panic"
        exit
fi

set -euox pipefail

source "./test/lib.include"

cat > "${PROJECT_PATH}/src/lib.rs" << EOF
use spacetimedb::{spacetimedb, println};
use std::cell::RefCell;

thread_local! {
    static X: RefCell<u32> = RefCell::new(0);
}
#[spacetimedb(reducer)]
fn first() {
    X.with(|x| {
        let x = x.borrow_mut();
        panic!()
    })
}
#[spacetimedb(reducer)]
fn second() {
    X.with(|x| *x.borrow_mut());
    println!("Test Passed");
}
EOF

run_test cargo run publish --skip_clippy --project-path "$PROJECT_PATH" --clear-database
[ "1" == "$(grep -c "reated new database" "$TEST_OUT")" ]
IDENT="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

set +e
cargo run call "$IDENT" first
set -e
run_test cargo run call "$IDENT" second

run_test cargo run logs "$IDENT"
[ ' Test Passed' == "$(grep 'Test Passed' "$TEST_OUT" | cut -d: -f6-)" ]
