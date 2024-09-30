from .. import Smoketest

class Permissions(Smoketest):
    AUTOPUBLISH = False

    def setUp(self):
        self.reset_config()

    def test_call(self):
        """Ensure that anyone has the permission to call any standard reducer"""

        identity = self.new_identity(email=None)
        token = self.token(identity)

        self.publish_module()

        # TODO: can a lot of the usage of reset_config be replaced with just passing -i ? or -a ?
        self.reset_config()
        self.new_identity(email=None)
        self.call("say_hello")

        self.reset_config()
        self.import_identity(identity, token, default=True)
        self.assertEqual("\n".join(self.logs(10000)).count("World"), 1)

    def test_delete(self):
        """Ensure that you cannot delete a database that you do not own"""

        identity = self.new_identity(email=None, default=True)

        self.publish_module()

        self.reset_config()
        with self.assertRaises(Exception):
            self.spacetime("delete", self.address)

    def test_describe(self):
        """Ensure that anyone can describe any database"""

        self.new_identity(email=None)
        self.publish_module()

        self.reset_config()
        self.new_identity(email=None)
        self.spacetime("describe", self.address)

    def test_logs(self):
        """Ensure that we are not able to view the logs of a module that we don't have permission to view"""

        self.new_identity(email=None)
        self.publish_module()

        self.reset_config()
        self.new_identity(email=None)
        self.call("say_hello")

        self.reset_config()
        identity = self.new_identity(email=None, default=True)
        with self.assertRaises(Exception):
            self.spacetime("logs", self.address, "10000")
    
    def test_publish(self):
        """This test checks to make sure that you cannot publish to an address that you do not own."""

        self.new_identity(email=None, default=True)
        self.publish_module()

        self.reset_config()

        with self.assertRaises(Exception):
            self.spacetime("publish", self.address, "--project-path", self.project_path, "--clear-database", "--yes")

        # Check that this holds without `--clear-database`, too.
        with self.assertRaises(Exception):
            self.spacetime("publish", self.address, "--project-path", self.project_path)


class PrivateTablePermissions(Smoketest):
    MODULE_CODE = """
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = secret)]
pub struct Secret {
    answer: u8,
}

#[spacetimedb::table(name = common_knowledge, public)]
pub struct CommonKnowledge {
    thing: String,
}

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.secret().insert(Secret { answer: 42 });
}

#[spacetimedb::reducer]
pub fn do_thing(ctx: &ReducerContext) {
    ctx.db.secret().insert(Secret { answer: 20 });
    ctx.db.common_knowledge().insert(CommonKnowledge { thing: "howdy".to_owned() });
}
"""

    def test_private_table(self):
        """Ensure that a private table can only be queried by the database owner"""

        out = self.spacetime("sql", self.address, "select * from secret")
        self.assertMultiLineEqual(out, """\
 answer 
--------
 42     
""")

        self.reset_config()
        self.new_identity(email=None)

        with self.assertRaises(Exception):
            self.spacetime("sql", self.address, "select * from secret")

        with self.assertRaises(Exception):
            self.subscribe("SELECT * FROM secret", n=0)

        sub = self.subscribe("SELECT * FROM *", n=1)
        self.call("do_thing", anon=True)
        self.assertEqual(sub(), [{'common_knowledge': {'deletes': [], 'inserts': [{'thing': 'howdy'}]}}])


class LifecycleReducers(Smoketest):
    def test_lifecycle_reducers_cant_be_called(self):
        """Ensure that reducers like __init__ can't be called"""

        with self.assertRaises(Exception):
            self.call("__init__")
        with self.assertRaises(Exception):
            self.call("__identity_connected__")
        with self.assertRaises(Exception):
            self.call("__identity_disconnected__")

