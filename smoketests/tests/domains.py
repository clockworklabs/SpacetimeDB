from .. import Smoketest, random_string
import unittest
import json

class Domains(Smoketest):
    AUTOPUBLISH = False

    def test_set_name(self):
        """Tests the functionality of the set-name command"""

        self.publish_module()

        rand_name = random_string()

        # This should throw an exception before there's a db with this name
        with self.assertRaises(Exception):
            self.spacetime("logs", rand_name)

        self.spacetime("rename", "--to", rand_name, self.database_identity)

        # Now we're essentially just testing that it *doesn't* throw an exception
        self.spacetime("logs", rand_name)

    @unittest.expectedFailure
    def test_subdomain_behavior(self):
        """Test how we treat the / character in published names"""

        root_name = random_string()
        self.publish_module(root_name)
        id_to_rename = self.database_identity

        self.publish_module(f"{root_name}/test")

        with self.assertRaises(Exception):
            self.publish_module(f"{root_name}//test")

        with self.assertRaises(Exception):
            self.publish_module(f"{root_name}/test/")

    def test_set_to_existing_name(self):
        """Test that we can't rename to a name already in use"""

        self.publish_module()
        id_to_rename = self.database_identity

        rename_to = random_string()
        self.publish_module(rename_to)

        # This should throw an exception because there's a db with this name
        with self.assertRaises(Exception):
            self.spacetime("rename", "--to", rename_to, id_to_rename)

    def test_replace_names(self):
        """Test that we can rename to a list of names"""

        orig_name = random_string()
        alt_name1 = random_string()
        alt_name2 = random_string()
        self.publish_module(orig_name)

        self.api_call(
            "PUT",
            f'/v1/database/{orig_name}/names',
            json.dumps([alt_name1, alt_name2]),
            {"Content-type": "application/json"}
        )

        # Use logs to check that name resolution works
        self.spacetime("logs", alt_name1)
        self.spacetime("logs", alt_name2)
        with self.assertRaises(Exception):
            self.spacetime("logs", orig_name)

        # Restore orig name so the database gets deleted on clean up
        self.api_call(
            "PUT",
            f'/v1/database/{alt_name1}/names',
            json.dumps([orig_name]),
            {"Content-type": "application/json"}
        )
