from .. import spacetime
import unittest
import tempfile

class CreateProject(unittest.TestCase):
    def test_create_project(self):
        """
        Ensure that the CLI is able to create a local project. This test does not depend on a running spacetimedb instance.
        """

        with tempfile.TemporaryDirectory() as tmpdir:
            with self.assertRaises(Exception):
                spacetime("init", "--non-interactive", "--name=test-project")
            with self.assertRaises(Exception):
                spacetime("init", "--non-interactive", "--name=test-project", tmpdir)
            spacetime("init", "--non-interactive", "--name=test-project", "--lang=rust", tmpdir)
            with self.assertRaises(Exception):
                spacetime("init", "--non-interactive", "--name=test-project", "--lang=rust", tmpdir)
