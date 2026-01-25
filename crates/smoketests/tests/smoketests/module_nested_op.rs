//! Nested table operation tests translated from smoketests/tests/module_nested_op.py

use spacetimedb_smoketests::Smoketest;

const MODULE_CODE: &str = r#"
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = account)]
pub struct Account {
    name: String,
    #[unique]
    id: i32,
}

#[spacetimedb::table(name = friends)]
pub struct Friends {
    friend_1: i32,
    friend_2: i32,
}

#[spacetimedb::reducer]
pub fn create_account(ctx: &ReducerContext, account_id: i32, name: String) {
    ctx.db.account().insert(Account { id: account_id, name } );
}

#[spacetimedb::reducer]
pub fn add_friend(ctx: &ReducerContext, my_id: i32, their_id: i32) {
    // Make sure our friend exists
    for account in ctx.db.account().iter() {
        if account.id == their_id {
            ctx.db.friends().insert(Friends { friend_1: my_id, friend_2: their_id });
            return;
        }
    }
}

#[spacetimedb::reducer]
pub fn say_friends(ctx: &ReducerContext) {
    for friendship in ctx.db.friends().iter() {
        let friend1 = ctx.db.account().id().find(&friendship.friend_1).unwrap();
        let friend2 = ctx.db.account().id().find(&friendship.friend_2).unwrap();
        log::info!("{} is friends with {}", friend1.name, friend2.name);
    }
}
"#;

/// This tests uploading a basic module and calling some functions and checking logs afterwards.
#[test]
fn test_module_nested_op() {
    let test = Smoketest::builder().module_code(MODULE_CODE).build();

    test.call("create_account", &["1", r#""House""#]).unwrap();
    test.call("create_account", &["2", r#""Wilson""#]).unwrap();
    test.call("add_friend", &["1", "2"]).unwrap();
    test.call("say_friends", &[]).unwrap();

    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("House is friends with Wilson")),
        "Expected 'House is friends with Wilson' in logs, got: {:?}",
        logs
    );
}
