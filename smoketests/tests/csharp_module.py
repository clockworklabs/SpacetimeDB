from .. import run_cmd, STDB_DIR, requires_dotnet, spacetime
import unittest
import tempfile
from pathlib import Path
import shutil
import subprocess


@requires_dotnet
class CreateProject(unittest.TestCase):
    def test_build_csharp_module(self):
        """
        Ensure that the CLI is able to create and compile a csharp project. This test does not depend on a running spacetimedb instance. Skips if dotnet 8.0 is not available
        """

        bindings = Path(STDB_DIR) / "crates" / "bindings-csharp"

        try:

            run_cmd("dotnet", "nuget", "locals", "all", "--clear", cwd=bindings, capture_stderr=True)
            run_cmd("dotnet", "workload", "install", "wasi-experimental")
            run_cmd("dotnet", "pack", cwd=bindings, capture_stderr=True)

            with tempfile.TemporaryDirectory() as tmpdir:
                spacetime("init", "--lang=csharp", tmpdir)

                packed_projects = ["BSATN.Runtime", "Runtime"]

                config = []
                config.append("<?xml version=\"1.0\" encoding=\"utf-8\"?>")
                config.append("<configuration>")
                config.append("<packageSources>")
                config.append("<!-- Local NuGet repositories -->")
                for project in packed_projects:
                    path = bindings / project / "bin" / "Release"
                    config.append("<add key=\"Local %s\" value=\"%s\" />\n" % (project, str(path)))
                config.append("<!-- Official NuGet.org server -->")
                config.append("<add key=\"NuGet.org\" value=\"https://api.nuget.org/v3/index.json\" />")
                config.append("</packageSources>")
                config.append("</configuration>")

                config = "\n".join(config)

                print("Writing `nuget.config` contents:")
                print(config)

                config_path = Path(tmpdir) / "nuget.config"
                with open(config_path, "w") as f:
                    f.write(config)

                run_cmd("dotnet", "publish", cwd=tmpdir, capture_stderr=True)

        except subprocess.CalledProcessError as e:
            print(e)
            print("output:")
            print(e.output)
            raise e
