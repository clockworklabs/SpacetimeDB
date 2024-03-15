from .. import Smoketest, random_string
import tempfile
import os


def count_matches(dir, needle):
    count = 0
    for f in os.listdir(dir):
        contents = open(os.path.join(dir, f)).read()
        count += contents.count(needle)
    return count

class Namespaces(Smoketest):
    AUTOPUBLISH = False

    def test_spacetimedb_ns_csharp(self):
        """Ensure that the default namespace is working properly"""

        namespace = "SpacetimeDB.Types"

        with tempfile.TemporaryDirectory() as tmpdir:
            self.spacetime("generate", "--out-dir", tmpdir, "--lang=cs", "--project-path", self.project_path)

            self.assertEqual(count_matches(tmpdir, f"namespace {namespace}"), 5)

    def test_custom_ns_csharp(self):
        """Ensure that when a custom namespace is specified on the command line, it actually gets used in generation"""

        namespace = random_string()

        with tempfile.TemporaryDirectory() as tmpdir:
            self.spacetime("generate", "--out-dir", tmpdir, "--lang=cs", "--namespace", namespace, "--project-path", self.project_path)

            self.assertEqual(count_matches(tmpdir, f"namespace {namespace}"), 5)
            self.assertEqual(count_matches(tmpdir, "using SpacetimeDB;"), 5)