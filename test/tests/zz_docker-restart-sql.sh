#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This tests to see if SpacetimeDB can be queried after a restart"
        exit
fi

set -euox pipefail

source "./test/lib.include"

create_project

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

run_test cargo run publish -s -d --project-path "$PROJECT_PATH" --clear-database
[ "1" == "$(grep -c "reated new database" "$TEST_OUT")" ]
IDENT="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

run_test cargo run call "$IDENT" add '["Robert"]'
run_test cargo run call "$IDENT" add '["Julie"]'
run_test cargo run call "$IDENT" add '["Samantha"]'
run_test cargo run call "$IDENT" say_hello
run_test cargo run logs "$IDENT" 100

[ ' INFO: src/lib.rs:16: Hello, Samantha!' == "$(grep 'Samantha' "$TEST_OUT" | tail -n 4)" ]
[ ' INFO: src/lib.rs:16: Hello, Julie!' == "$(grep 'Julie' "$TEST_OUT" | tail -n 4)" ]
[ ' INFO: src/lib.rs:16: Hello, Robert!' == "$(grep 'Robert' "$TEST_OUT" | tail -n 4)" ]
[ ' INFO: src/lib.rs:18: Hello, World!' == "$(grep 'World' "$TEST_OUT" | tail -n 4)" ]

CONTAINER_NAME=$(docker ps | grep node | awk '{print $NF}')
run_test docker kill $CONTAINER_NAME
run_test cargo build -p spacetimedb-standalone --release
run_test docker-compose start node
sleep 10
run_test cargo run sql "${IDENT}" "SELECT * FROM Person"
[ 'Robert' == "$(grep 'Robert' "$TEST_OUT" | awk '{$1=$1};1')" ]
