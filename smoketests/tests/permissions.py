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

    def test_describe(self):
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
            self.spacetime("publish", self.address, "--project-path", self.project_path, "--clear-database", "--force")

    
class PrivateTablePermissions(Smoketest):
    MODULE_CODE = """
use spacetimedb::spacetimedb;

#[spacetimedb(table)]
#[sats(name = "_Secret")]
pub struct Secret {
    answer: u8,
}

#[spacetimedb(init)]
pub fn init() {
    Secret::insert(Secret { answer: 42 });
}
"""

    def test_private_table(self):
        """Ensure that a private table can only be queried by the database owner"""

        out = self.spacetime("sql", self.address, "select * from _Secret")
        self.assertMultiLineEqual(out, """\
 answer 
--------
 42     
""")

        self.reset_config()
        self.new_identity(email=None)

        with self.assertRaises(Exception):
            self.spacetime("sql", self.address, "select * from _Secret")

