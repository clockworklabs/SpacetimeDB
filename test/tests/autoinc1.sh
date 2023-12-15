#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This tests the autoinc functionality"
        exit
fi

set -euox pipefail

source "./test/lib.include"

do_test() {
  echo "RUNNING TEST FOR VALUE: $1"
  reset_project

  cat > "${PROJECT_PATH}/src/lib.rs" << EOF
use spacetimedb::{println, spacetimedb};

#[spacetimedb(table)]
pub struct Person {
    #[autoinc]
    key_col: REPLACE_VALUE,
    name: String,
}

#[spacetimedb(reducer)]
pub fn add(name: String, expected_value: REPLACE_VALUE) {
    let value = Person::insert(Person { key_col: 0, name });
    assert_eq!(value.key_col, expected_value);
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

  run_test cargo run publish --project-path "$PROJECT_PATH" --clear-database --skip_clippy
  [ "1" == "$(grep -c "reated new database" "$TEST_OUT")" ]
  IDENT="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

  run_test cargo run call "$IDENT" add Robert 1
  run_test cargo run call "$IDENT" add Julie 2
  run_test cargo run call "$IDENT" add Samantha 3
  run_test cargo run call "$IDENT" say_hello
  run_test cargo run logs "$IDENT" 100
  [[ "$(grep 'Samantha' "$TEST_OUT" | tail -n 4)" =~ .*Hello,\ 3:Samantha! ]]
  [[ "$(grep 'Julie' "$TEST_OUT" | tail -n 4)" =~ .*Hello,\ 2:Julie! ]]
  [[ "$(grep 'Robert' "$TEST_OUT" | tail -n 4)" =~ .*Hello,\ 1:Robert! ]]
  [[ "$(grep 'World' "$TEST_OUT" | tail -n 4)" =~ .*Hello,\ World! ]]

  clear_project
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
