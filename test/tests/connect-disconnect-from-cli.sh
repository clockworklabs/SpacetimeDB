#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This makes sure that the connect and disconnect functions are called when invoking a reducer from the CLI"
        exit
fi

set -euox pipefail

source "./test/lib.include"

cat > "${PROJECT_PATH}/src/lib.rs" << EOF
use spacetimedb::{println, spacetimedb, ReducerContext};

#[spacetimedb(connect)]
pub fn connected(_ctx: ReducerContext) {
    println!("_connect called");
    panic!("Panic on connect");
}

#[spacetimedb(disconnect)]
pub fn disconnected(_ctx: ReducerContext) {
    println!("disconnect called");
    panic!("Panic on disconnect");
}

#[spacetimedb(reducer)]
pub fn say_hello() {
    println!("Hello, World!");
}
EOF

run_test cargo run publish --skip_clippy --project-path "$PROJECT_PATH" --clear-database
[ "1" == "$(grep -c "reated new database" "$TEST_OUT")" ]
IDENT="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

run_test cargo run call "$IDENT" say_hello
run_test cargo run logs "$IDENT"
[ ' _connect called' == "$(grep '_connect called' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]
[ ' disconnect called' == "$(grep 'disconnect called' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]
[ ' Hello, World!' == "$(grep 'Hello, World!' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]
