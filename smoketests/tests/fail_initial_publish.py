from .. import Smoketest, random_string
import subprocess

class FailInitialPublish(Smoketest):
    AUTOPUBLISH = False

    MODULE_CODE_BROKEN = """
use spacetimedb::{client_visibility_filter, Filter};

#[spacetimedb::table(name = person)]
pub struct Person {
    name: String,
}

#[client_visibility_filter]
// Bug: `Person` is the wrong table name, should be `person`.
const HIDE_PEOPLE_EXCEPT_ME: Filter = Filter::Sql("SELECT * FROM Person WHERE name = 'me'");
"""

    MODULE_CODE_FIXED = """
use spacetimedb::{client_visibility_filter, Filter};

#[spacetimedb::table(name = person)]
pub struct Person {
    name: String,
}

#[client_visibility_filter]
const HIDE_PEOPLE_EXCEPT_ME: Filter = Filter::Sql("SELECT * FROM person WHERE name = 'me'");
"""

    FIXED_QUERY = '"sql": "SELECT * FROM person WHERE name = \'me\'"'

    def test_fail_initial_publish(self):
        """This tests that publishing an invalid module does not leave a broken entry in the control DB."""

        name = random_string()

        self.write_module_code(self.MODULE_CODE_BROKEN)

        with self.assertRaises(Exception):
            self.publish_module(name)

        describe_output = self.spacetime("describe", "--json", name, full_output = True, check = False)

        with self.assertRaises(subprocess.CalledProcessError):
            describe_output.check_returncode()

        self.assertIn("Error: No such database.", describe_output.stderr)

        # We can publish a fixed module under the same database name.
        # This used to be broken;
        # the failed initial publish would leave the control database in a bad state.
        self.write_module_code(self.MODULE_CODE_FIXED)

        self.publish_module(name, clear = False)
        describe_output = self.spacetime("describe", "--json", name)

        self.assertIn(
            self.FIXED_QUERY,
            [line.strip() for line in describe_output.splitlines()],
        )

        # Publishing the broken code again fails, but the database still exists afterwards,
        # with the previous version of the module code.
        self.write_module_code(self.MODULE_CODE_BROKEN)

        with self.assertRaises(Exception):
            self.publish_module(name, clear = False)

        describe_output = self.spacetime("describe", "--json", name)
        self.assertIn(
            self.FIXED_QUERY,
            [line.strip() for line in describe_output.splitlines()],
        )
