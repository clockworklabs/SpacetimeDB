from .. import Smoketest

class ConnDisconnFromCli(Smoketest):

    MODULE_CODE = """
use spacetimedb::{log, ReducerContext};

#[spacetimedb::reducer(client_connected)]
pub fn connected(_ctx: &ReducerContext) {
    log::info!("_connect called");
}

#[spacetimedb::reducer(client_disconnected)]
pub fn disconnected(_ctx: &ReducerContext) {
    log::info!("disconnect called");
}

#[spacetimedb::reducer]
pub fn say_hello(_ctx: &ReducerContext) {
    log::info!("Hello, World!");
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
