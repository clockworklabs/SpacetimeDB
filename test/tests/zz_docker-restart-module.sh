#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This tests to see if a SpacetimeDB module's reducer can still be invoked after a restart"
        exit
fi

set -euox pipefail

source "./test/lib.include"

# Note: creating indexes on `Person`
# exercises more possible failure cases when replaying after restart
cat > "${PROJECT_PATH}/src/lib.rs" << EOF
use spacetimedb::{println, spacetimedb};

#[spacetimedb(table)]
#[spacetimedb(index(btree, name = "name_idx", name))]
pub struct Person {
    #[primarykey]
    #[autoinc]
    id: u32,
    name: String,
}

#[spacetimedb(reducer)]
pub fn add(name: String) {
Person::insert(Person { id: 0, name }).unwrap();
}

#[spacetimedb(reducer)]
pub fn say_hello() {
    for person in Person::iter() {
        println!("Hello, {}!", person.name);
    }
    println!("Hello, World!");
}
EOF

run_test cargo run publish --skip_clippy --project-path "$PROJECT_PATH" --clear-database
[ "1" == "$(grep -c "reated new database" "$TEST_OUT")" ]
IDENT="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"
run_test cargo run call "$IDENT" add Robert

restart_docker
run_test cargo run call "$IDENT" add Julie
run_test cargo run call "$IDENT" add Samantha
run_test cargo run call "$IDENT" say_hello
run_test cargo run logs "$IDENT" 100

[ ' Hello, Samantha!' == "$(grep 'Samantha' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]
[ ' Hello, Julie!' == "$(grep 'Julie' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]
[ ' Hello, Robert!' == "$(grep 'Robert' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]
[ ' Hello, World!' == "$(grep 'World' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]
