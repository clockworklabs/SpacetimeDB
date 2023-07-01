#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This tests unique constraints being violated during autoinc insertion"
        exit
fi

set -euox pipefail

source "./test/lib.include"

do_test() {
  echo "RUNNING TEST FOR VALUE: $1"
  reset_project

  cat > "${PROJECT_PATH}/src/lib.rs" << EOF
use std::error::Error;
use spacetimedb::{println, spacetimedb};

#[spacetimedb(table)]
pub struct Person {
    #[autoinc]
    #[unique]
    key_col: REPLACE_VALUE,
    #[unique]
    name: String,
}

#[spacetimedb(reducer)]
pub fn add_new(name: String) -> Result<(), Box<dyn Error>> {
    let value = Person::insert(Person { key_col: 0, name })?;
    println!("Assigned Value: {} -> {}", value.key_col, value.name);
    Ok(())
}

#[spacetimedb(reducer)]
pub fn update(name: String, new_id: REPLACE_VALUE) {
    Person::delete_by_name(&name);
    let _value = Person::insert(Person { key_col: new_id, name });
}

#[spacetimedb(reducer)]
pub fn say_hello() {
    for person in Person::iter() {
        println!("Hello, {}:{}!", person.key_col, person.name);
    }
    println!("Hello, World!");
}
EOF

  fsed "s/REPLACE_VALUE/$1/g" "${PROJECT_PATH}/src/lib.rs"

  run_test spacetime publish --project-path "$PROJECT_PATH" --clear-database
  [ "1" == "$(grep -c "reated new database" "$TEST_OUT")" ]
  IDENT="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

  run_test spacetime call "$IDENT" update '["Robert", 2]'
  run_test spacetime call "$IDENT" add_new '["Success"]'
  if run_test spacetime call "$IDENT" add_new '["Failure"]' ; then
    # This add_new call should have failed. Its possible there was a duplicate insert
    spacetime logs "$IDENT"
    spacetime sql "$IDENT" 'SELECT * FROM Person'
    exit 1
  fi

  run_test spacetime call "$IDENT" say_hello
  run_test spacetime logs "$IDENT" 100
  [[ "$(grep 'Robert' "$TEST_OUT" | tail -n 4)" =~ .*Hello,\ 2:Robert! ]]
  [[ "$(grep 'Success' "$TEST_OUT" | tail -n 4)" =~ .*Hello,\ 1:Success! ]]
  [[ "$(grep 'World' "$TEST_OUT" | tail -n 4)" =~ .*Hello,\ World! ]]
}

do_test u8
do_test i8
do_test u16
do_test i16
do_test u32
do_test i32
do_test u64
do_test i64
do_test u128
do_test i128
