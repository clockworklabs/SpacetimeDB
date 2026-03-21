from .. import Smoketest

MODULE_HEADER = """
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = all_u8s, public)]
pub struct AllU8s {
    number: u8,
}

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    // Here's a bunch of data that no one will be able to subscribe to.
    for i in u8::MIN..=u8::MAX {
        ctx.db.all_u8s().insert(AllU8s { number: i });
    }
}
"""

class ClientConnectedErrorRejectsConnection(Smoketest):
    MODULE_CODE = MODULE_HEADER + """

#[spacetimedb::reducer(client_connected)]
pub fn identity_connected(ctx: &ReducerContext) -> Result<(), String> {
     Err("Rejecting connection from client".to_string())
}

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(_ctx: &ReducerContext) {
    panic!("This should never be called, since we reject all connections!")
}
"""

    def test_client_connected_error_rejects_connection(self):
        with self.assertRaises(Exception):
            self.subscribe("select * from all_u8s", n = 0)()

        logs = self.logs(100)
        self.assertIn('Rejecting connection from client', logs)
        self.assertNotIn('This should never be called, since we reject all connections!', logs)

class ClientDisconnectedErrorStillDeletesStClient(Smoketest):
    MODULE_CODE = MODULE_HEADER + """
#[spacetimedb::reducer(client_connected)]
pub fn identity_connected(_ctx: &ReducerContext) -> Result<(), String> {
     Ok(())
}

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(_ctx: &ReducerContext) {
    panic!("This should be called, but the `st_client` row should still be deleted")
}
"""

    def test_client_disconnected_error_still_deletes_st_client(self):
        self.subscribe("select * from all_u8s", n = 0)()

        logs = self.logs(100)
        self.assertIn('This should be called, but the `st_client` row should still be deleted', logs)

        sql_out = self.spacetime("sql", self.database_identity, "select * from st_client")

        # The SQL query itself now creates a temporary connection, so we may
        # see exactly one row (the SQL connection's own). The websocket's row
        # should be gone. Count non-header, non-separator lines with content.
        lines = sql_out.strip().split('\n')
        # Data rows are those that are not the header and not the separator line
        data_rows = [l for l in lines if '|' in l and '-+-' not in l and 'identity' not in l.lower()]
        self.assertLessEqual(len(data_rows), 1,
            f"Expected at most 1 st_client row (the SQL connection itself), got: {sql_out}")
