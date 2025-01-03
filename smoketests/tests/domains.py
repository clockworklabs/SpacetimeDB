from .. import Smoketest, random_string

class Domains(Smoketest):
    AUTOPUBLISH = False

    def test_set_name(self):
        """Tests the functionality of the set-name command"""

        self.publish_module()

        rand_name = random_string()

        # This should throw an exception before there's a db with this name
        with self.assertRaises(Exception):
            self.spacetime("logs", rand_name)

        self.spacetime("rename", rand_name, self.database_identity)

        # Now we're essentially just testing that it *doesn't* throw an exception
        self.spacetime("logs", rand_name)
