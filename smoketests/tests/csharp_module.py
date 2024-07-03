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
                restore_sources = [str(bindings / project / "bin" / "Release") for project in packed_projects]

                csproj = Path(tmpdir) / "StdbModule.csproj"
                with open(csproj, "r") as f:
                    contents = f.read()

                contents = contents.replace(
                    "</PropertyGroup>",
                    # note that nuget URL comes last, which ensures local sources should override it.
                    f"""<RestoreSources>{str.join(";", restore_sources)}</RestoreSources>
</PropertyGroup>""",
                )
                with open(csproj, "w") as f:
                    f.write(contents)

                run_cmd("dotnet", "publish", cwd=tmpdir, capture_stderr=True)

        except subprocess.CalledProcessError as e:
            print(e)
            print("output:")
            print(e.output)
            raise e
