from .. import Smoketest, random_string
import json

class Permissions(Smoketest):
    AUTOPUBLISH = False

    def setUp(self):
        self.reset_config()

    def test_call(self):
        """Ensure that anyone has the permission to call any standard reducer"""

        self.publish_module()

        self.call("say_hello", anon=True)

        self.assertEqual("\n".join(self.logs(10000)).count("World"), 1)

    def test_delete(self):
        """Ensure that you cannot delete a database that you do not own"""

        self.publish_module()

        self.new_identity()
        with self.assertRaises(Exception):
            self.spacetime("delete", self.database_identity)

    def test_describe(self):
        """Ensure that anyone can describe any database"""

        self.publish_module()

        self.spacetime("describe", "--anonymous", "--json", self.database_identity)

    def test_logs(self):
        """Ensure that we are not able to view the logs of a module that we don't have permission to view"""

        self.publish_module()

        self.reset_config()
        self.new_identity()
        self.call("say_hello")

        self.reset_config()
        self.new_identity()
        with self.assertRaises(Exception):
            self.spacetime("logs", self.database_identity, "-n", "10000")

    def test_publish(self):
        """This test checks to make sure that you cannot publish to an identity that you do not own."""

        self.publish_module()

        self.new_identity()

        with self.assertRaises(Exception):
            self.spacetime("publish", self.database_identity, "--project-path", self.project_path, "--delete-data", "--yes")

        # Check that this holds without `--delete-data`, too.
        with self.assertRaises(Exception):
            self.spacetime("publish", self.database_identity, "--project-path", self.project_path, "--yes")

    def test_replace_names(self):
        """Test that you can't replace names of a database you don't own"""

        name = random_string()
        self.publish_module(name)

        self.new_identity()

        with self.assertRaises(Exception):
            self.api_call(
                "PUT",
                f'/v1/database/{name}/names',
                json.dumps(["post", "gres"]),
                {"Content-type": "application/json"}
            )

class PrivateTablePermissions(Smoketest):
    MODULE_CODE = """
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = secret, private)]
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
pub fn do_thing(ctx: &ReducerContext, thing: String) {
    ctx.db.secret().insert(Secret { answer: 20 });
    ctx.db.common_knowledge().insert(CommonKnowledge { thing });
}
"""

    def test_private_table(self):
        """Ensure that a private table can only be queried by the database owner"""

        out = self.spacetime("sql", self.database_identity, "select * from secret")
        answer = "\n".join([
            " answer ",
            "--------",
            " 42     ",
            ""
        ])
        self.assertMultiLineEqual(str(out), answer)

        self.reset_config()
        self.new_identity()

        with self.assertRaises(Exception):
            self.spacetime("sql", self.database_identity, "select * from secret")

        # Subscribing to the private table failes.
        with self.assertRaises(Exception):
            self.subscribe("SELECT * FROM secret", n=0)

        # Subscribing to the public table works.
        sub = self.subscribe("SELECT * FROM common_knowledge", n = 1)
        self.call("do_thing", "godmorgon")
        self.assertEqual(sub(), [
            {
                'common_knowledge': {
                    'deletes': [],
                    'inserts': [{'thing': 'godmorgon'}]
                }
            }
        ])

        # Subscribing to both tables returns updates for the public one.
        sub = self.subscribe("SELECT * FROM *", n=1)
        self.call("do_thing", "howdy", anon=True)
        self.assertEqual(sub(), [
            {
                'common_knowledge': {
                    'deletes': [],
                    'inserts': [{'thing': 'howdy'}]
                }
            }
        ])


class LifecycleReducers(Smoketest):
    lifecycle_kinds = "init", "client_connected", "client_disconnected"

    MODULE_CODE = "\n".join(f"""
#[spacetimedb::reducer({kind})]
fn lifecycle_{kind}(_ctx: &spacetimedb::ReducerContext) {{}}
""" for kind in lifecycle_kinds)

    def test_lifecycle_reducers_cant_be_called(self):
        """Ensure that lifecycle reducers (init, on_connect, etc) can't be called"""

        for kind in self.lifecycle_kinds:
            with self.assertRaises(Exception):
                self.call(f"lifecycle_{kind}")

