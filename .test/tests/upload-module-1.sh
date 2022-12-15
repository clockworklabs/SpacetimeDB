#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This tests uploading a basic module and calling some functions and checking logs afterwards."
        exit
fi

set -euox pipefail

source "./.test/lib.include"

create_project

cat > "${PROJECT_PATH}/src/lib.rs" << EOF
use spacetimedb::{println, spacetimedb, Hash};

#[spacetimedb(table)]
pub struct Person {
    name: String,
}

#[spacetimedb(reducer)]
pub fn add(_sender: Hash, _timestamp: u64, name: String) {
    Person::insert(Person { name })
}

#[spacetimedb(reducer)]
pub fn say_hello(_sender: Hash, _timestamp: u64) {
    for person in Person::iter() {
        println!("Hello, {}!", person.name);
    }
    println!("Hello, World!");
}
EOF

spacetime_publish --project-path "$PROJECT_PATH"
[ "1" == "$(grep -c "reated new database" "$TEST_OUT")" ]
IDENT="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

run_test cargo run call "$IDENT" add '["Robert"]'
run_test cargo run call "$IDENT" add '["Julie"]'
run_test cargo run call "$IDENT" add '["Samantha"]'
run_test cargo run call "$IDENT" say_hello
run_test cargo run logs "$IDENT" 100
[ ' info: Hello, Samantha!' == "$(grep 'Samantha' "$TEST_OUT" | tail -n 4)" ]
[ ' info: Hello, Julie!' == "$(grep 'Julie' "$TEST_OUT" | tail -n 4)" ]
[ ' info: Hello, Robert!' == "$(grep 'Robert' "$TEST_OUT" | tail -n 4)" ]
[ ' info: Hello, World!' == "$(grep 'World' "$TEST_OUT" | tail -n 4)" ]
