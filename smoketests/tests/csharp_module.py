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
        codegen = bindings / "Codegen"
        runtime = bindings / "Runtime"

        try:

            run_cmd("dotnet", "workload", "install", "wasi-experimental")
            run_cmd("dotnet", "pack", cwd=codegen, capture_stderr=True)
            run_cmd("dotnet", "pack", cwd=runtime, capture_stderr=True)

            with tempfile.TemporaryDirectory() as tmpdir:
                spacetime("init", "--lang=csharp", tmpdir)

                codegen_bin = codegen / "bin" / "Release"
                runtime_bin = runtime / "bin" / "Release"

                csproj = Path(tmpdir) / "StdbModule.csproj"
                with open(csproj, "r") as f:
                    contents = f.read()

                contents = contents.replace(
                    "</PropertyGroup>",
                    # note that nuget URL comes last, which ensures local sources should override it.
                    f"""<RestoreSources>{codegen_bin.absolute()};{runtime_bin.absolute()};https://api.nuget.org/v3/index.json</RestoreSources>
</PropertyGroup>""",
                )
                with open(csproj, "w") as f:
                    f.write(contents)

                run_cmd("dotnet", "build", cwd=tmpdir, capture_stderr=True)

        except subprocess.CalledProcessError as e:
            print(e)
            print("output:")
            print(e.output)
            raise e
