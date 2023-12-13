#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This tests uploading a basic module and calling some functions and checking logs afterwards."
        exit
fi

set -euox pipefail

source "./test/lib.include"

cat > "${PROJECT_PATH}/src/lib.rs" << EOF
use spacetimedb::{println, spacetimedb};

#[spacetimedb(table)]
pub struct Account {
    name: String,
    #[unique]
    id: i32,
}

#[spacetimedb(table)]
pub struct Friends {
    friend_1: i32,
    friend_2: i32,
}

#[spacetimedb(reducer)]
pub fn create_account(account_id: i32, name: String) {
    Account::insert(Account { id: account_id, name } );
}

#[spacetimedb(reducer)]
pub fn add_friend(my_id: i32, their_id: i32) {

    // Make sure our friend exists
    for account in Account::iter() {
        if account.id == their_id {
	    Friends::insert(Friends { friend_1: my_id, friend_2: their_id } );
            return;
	}
    }
}

#[spacetimedb(reducer)]
pub fn say_friends() {
    for friendship in Friends::iter() {
	let friend1 = Account::filter_by_id(&friendship.friend_1).unwrap();
	let friend2 = Account::filter_by_id(&friendship.friend_2).unwrap();
	println!("{} is friends with {}", friend1.name, friend2.name);
    }
}
EOF

run_test cargo run publish --skip_clippy --project-path "$PROJECT_PATH" --clear-database
[ "1" == "$(grep -c "reated new database" "$TEST_OUT")" ]
IDENT="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

run_test cargo run call "$IDENT" create_account 1 House
run_test cargo run call "$IDENT" create_account 2 Wilson
run_test cargo run call "$IDENT" add_friend 1 2
run_test cargo run call "$IDENT" say_friends
run_test cargo run logs "$IDENT" 100
[ ' House is friends with Wilson' == "$(grep 'House' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]
