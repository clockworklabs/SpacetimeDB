from .. import Smoketest

class Dml(Smoketest):
    MODULE_CODE = """
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = t, public)]
pub struct T {
    name: String,
}
"""

    def test_subscribe(self):
        """Test that we receive subscription updates from DML"""

        # Subscribe to `t`
        sub = self.subscribe("SELECT * FROM t", n=2)

        self.spacetime("sql", self.database_identity, "INSERT INTO t (name) VALUES ('Alice')")
        self.spacetime("sql", self.database_identity, "INSERT INTO t (name) VALUES ('Bob')")

        self.assertEqual(
            sub(),
            [
                {"t": {"deletes": [], "inserts": [{"name": "Alice"}]}},
                {"t": {"deletes": [], "inserts": [{"name": "Bob"}]}},
            ],
        )
