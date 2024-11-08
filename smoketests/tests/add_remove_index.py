from .. import Smoketest, random_string

class AddRemoveIndex(Smoketest):
    AUTOPUBLISH = False

    MODULE_CODE = """
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = t1)]
pub struct T1 { id: u64 }

#[spacetimedb::table(name = t2)]
pub struct T2 { id: u64 }

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    for id in 0..1_000 {
        ctx.db.t1().insert(T1 { id });
        ctx.db.t2().insert(T2 { id });
    }
}
"""
    MODULE_CODE_INDEXED = """
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = t1)]
pub struct T1 { #[index(btree)] id: u64 }

#[spacetimedb::table(name = t2)]
pub struct T2 { #[index(btree)] id: u64 }

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    for id in 0..1_000 {
        ctx.db.t1().insert(T1 { id });
        ctx.db.t2().insert(T2 { id });
    }
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext) {
    let id = 1_001;
    ctx.db.t1().insert(T1 { id });
    ctx.db.t2().insert(T2 { id });
}
"""

    JOIN_QUERY = "select t1.* from t1 join t2 on t1.id = t2.id where t2.id = 1001"

    def between_publishes(self):
        """
        The test `AddRemoveIndexAfterRestart` in `zz_docker.py`
        overwrites this method to restart docker between each publish,
        otherwise reusing this test's code.
        """
        pass

    def test_add_then_remove_index(self):
        """
        First publish without the indices,
        then add the indices, and publish,
        and finally remove the indices, and publish again.
        There should be no errors
        and the unindexed versions should reject subscriptions.
        """

        name = random_string()

        # Publish and attempt a subscribing to a join query.
        # There are no indices, resulting in an unsupported unindexed join.
        self.publish_module(name, clear = False)
        with self.assertRaises(Exception):
            self.subscribe(self.JOIN_QUERY, n = 0)

        self.between_publishes()

        # Publish the indexed version.
        # Now we have indices, so the query should be accepted.
        self.write_module_code(self.MODULE_CODE_INDEXED)
        self.publish_module(name, clear = False)
        sub = self.subscribe(self.JOIN_QUERY, n = 1)
        self.call("add", anon = True)
        self.assertEqual(sub(), [{'t1': {'deletes': [], 'inserts': [{'id': 1001}]}}])

        self.between_publishes()

        # Publish the unindexed version again, removing the index.
        # The initial subscription should be rejected again.
        self.write_module_code(self.MODULE_CODE)
        self.publish_module(name, clear = False)
        with self.assertRaises(Exception):
            self.subscribe(self.JOIN_QUERY, n = 0)
