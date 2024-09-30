from .. import Smoketest

class ModuleNestedOp(Smoketest):
    MODULE_CODE = """
use spacetimedb::{println, ReducerContext, Table};

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
        println!("{} is friends with {}", friend1.name, friend2.name);
    }
}
"""

    def test_module_nested_op(self):
        """This tests uploading a basic module and calling some functions and checking logs afterwards."""

        self.call("create_account", 1, "House")
        self.call("create_account", 2, "Wilson")
        self.call("add_friend", 1, 2)
        self.call("say_friends")
        self.assertIn("House is friends with Wilson", self.logs(2))
