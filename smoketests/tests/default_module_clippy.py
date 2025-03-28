import unittest
import tempfile
import subprocess
from .. import Smoketest

class ClippyDefaultModule(Smoketest):
    AUTOPUBLISH = False

    def test_default_module_clippy_check(self):
        """Ensure that the default rust module has no clippy errors or warnings"""

        subprocess.check_call(["cargo", "clippy", "--", "-Dwarnings"], cwd=self.project_path)