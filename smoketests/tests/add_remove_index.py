from .. import Smoketest, random_string

class AddRemoveIndex(Smoketest):
    AUTOPUBLISH = False

    MODULE_CODE = """
use spacetimedb::spacetimedb;

#[spacetimedb(table)]
pub struct T1 { id: u64 }

#[spacetimedb(table)]
pub struct T2 { id: u64 }

#[spacetimedb(init)]
pub fn init() {
    for id in 0..1_000 {
        T1::insert(T1 { id });
        T2::insert(T2 { id });
    }
}
"""
    MODULE_CODE_INDEXED = """
use spacetimedb::spacetimedb;

#[spacetimedb(table)]
#[spacetimedb(index(btree, name = "id", id))]
pub struct T1 { id: u64 }

#[spacetimedb(table)]
#[spacetimedb(index(btree, name = "id", id))]
pub struct T2 { id: u64 }

#[spacetimedb(init)]
pub fn init() {
    for id in 0..1_000 {
        T1::insert(T1 { id });
        T2::insert(T2 { id });
    }
}

#[spacetimedb(reducer)]
pub fn add() {
    let id = 1_001;
    T1::insert(T1 { id });
    T2::insert(T2 { id });
}
"""

    JOIN_QUERY = "select T1.* from T1 join T2 on T1.id = T2.id where T2.id = 1001"

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
            self.subscribe(self.JOIN_QUERY, n = 0)()

        # Publish the indexed version.
        # Now we have indices, so the query should be accepted.
        self.write_module_code(self.MODULE_CODE_INDEXED)
        self.publish_module(name, clear = False)
        sub = self.subscribe(self.JOIN_QUERY, n = 1)
        self.call("add", anon = True)
        self.assertEqual(sub(), [{'T1': {'deletes': [], 'inserts': [{'id': 1001}]}}])

        # Publish the unindexed version again, removing the index.
        # The initial subscription should be rejected again.
        self.write_module_code(self.MODULE_CODE)
        self.publish_module(name, clear = False)
        with self.assertRaises(Exception):
            self.subscribe(self.JOIN_QUERY, n = 0)()
