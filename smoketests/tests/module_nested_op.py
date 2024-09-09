from .. import Smoketest

class ModuleNestedOp(Smoketest):
    MODULE_CODE = """
use spacetimedb::println;

#[spacetimedb::table(name = accounts)]
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
pub fn create_account(account_id: i32, name: String) {
    Account::insert(Account { id: account_id, name } );
}

#[spacetimedb::reducer]
pub fn add_friend(my_id: i32, their_id: i32) {
    // Make sure our friend exists
    for account in Account::iter() {
        if account.id == their_id {
            Friends::insert(Friends { friend_1: my_id, friend_2: their_id });
            return;
        }
    }
}

#[spacetimedb::reducer]
pub fn say_friends() {
    for friendship in Friends::iter() {
        let friend1 = Account::filter_by_id(&friendship.friend_1).unwrap();
        let friend2 = Account::filter_by_id(&friendship.friend_2).unwrap();
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
