#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test is designed to test the entirety of the new user flow."
        exit
fi

set -euox pipefail

source "./test/lib.include"

cargo run identity new --no-email

## Write a spacetimedb rust module
cat > "${PROJECT_PATH}/src/lib.rs" <<EOF
use spacetimedb::{spacetimedb, println};

#[spacetimedb(table)]
pub struct Person {
    name: String
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

## Publish your module
run_test cargo run publish --skip_clippy --project-path "$PROJECT_PATH" --clear-database
ADDRESS="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

# We have to give the database some time to setup our instance
sleep 2

# Calling our database
run_test cargo run call "$ADDRESS" say_hello
run_test cargo run logs "$ADDRESS"
if [ "$(grep -c "Hello, World!" "$TEST_OUT")" != 1 ]; then exit 1; fi

## Calling functions with arguments
run_test cargo run call "$ADDRESS" add Tyler
run_test cargo run call "$ADDRESS" say_hello
run_test cargo run logs "$ADDRESS"

[ "$(grep -c "Hello, World!" "$TEST_OUT")" == 2 ]
[ "$(grep -c "Hello, Tyler!" "$TEST_OUT")" == 1 ]

run_test cargo run sql "$ADDRESS" "SELECT * FROM Person"
[ "$(tail -n 3 "$TEST_OUT")" == \
' name  '$'\n'\
'-------'$'\n'\
' Tyler ' ]
