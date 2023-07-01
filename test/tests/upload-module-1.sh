#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This tests uploading a basic module and calling some functions and checking logs afterwards."
        exit
fi

set -euox pipefail

source "./test/lib.include"

reset_project

cat > "${PROJECT_PATH}/src/lib.rs" << EOF
use spacetimedb::{println, spacetimedb};

#[spacetimedb(table)]
pub struct Person {
    name: String,
}

#[spacetimedb(reducer)]
pub fn add(name: String) {
    Person::insert(Person { name });
}

#[spacetimedb(reducer)]
pub fn say_hello() {
    for person in Person::iter() {
        println!("Hello, {}!", person.name);
    }
    println!("Hello, World!");
}
EOF

run_test spacetime publish -s -d --project-path "$PROJECT_PATH" --clear-database
[ "1" == "$(grep -c "reated new database" "$TEST_OUT")" ]
IDENT="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

run_test spacetime call "$IDENT" add '["Robert"]'
run_test spacetime call "$IDENT" add '["Julie"]'
run_test spacetime call "$IDENT" add '["Samantha"]'
run_test spacetime call "$IDENT" say_hello
run_test spacetime logs "$IDENT" 100
[ ' Hello, Samantha!' == "$(grep 'Samantha' "$TEST_OUT" | tail -n 4 | cut -d: -f4-)" ]
[ ' Hello, Julie!' == "$(grep 'Julie' "$TEST_OUT" | tail -n 4 | cut -d: -f4-)" ]
[ ' Hello, Robert!' == "$(grep 'Robert' "$TEST_OUT" | tail -n 4 | cut -d: -f4-)" ]
[ ' Hello, World!' == "$(grep 'World' "$TEST_OUT" | tail -n 4 | cut -d: -f4-)" ]
