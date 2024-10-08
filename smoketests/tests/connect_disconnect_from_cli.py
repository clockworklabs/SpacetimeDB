from .. import Smoketest

class ConnDisconnFromCli(Smoketest):

    MODULE_CODE = """
use spacetimedb::{println, ReducerContext};

#[spacetimedb::reducer(client_connected)]
pub fn connected(_ctx: &ReducerContext) {
    println!("_connect called");
    panic!("Panic on connect");
}

#[spacetimedb::reducer(client_disconnected)]
pub fn disconnected(_ctx: &ReducerContext) {
    println!("disconnect called");
    panic!("Panic on disconnect");
}

#[spacetimedb::reducer]
pub fn say_hello(_ctx: &ReducerContext) {
    println!("Hello, World!");
}
"""

    def test_conn_disconn(self):
        """
        Ensure that the connect and disconnect functions are called when invoking a reducer from the CLI
        """

        self.call("say_hello")
        logs = self.logs(10)
        self.assertIn('_connect called', logs)
        self.assertIn('disconnect called', logs)
        self.assertIn('Hello, World!', logs)
