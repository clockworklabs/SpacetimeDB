from .. import run_cmd, STDB_DIR, requires_dotnet, spacetime
import unittest
import tempfile
from pathlib import Path
import shutil
import subprocess
import re
from typing import List, Tuple


class CSProjVersionOverride:
    csproj: Path
    before_version: str
    temp_version: str

    def __init__(self, csproj: Path, temp_version: str) -> None:
        """
        Use this class as a context manager to temporarily override the version of a .csproj file. The version will be restored when the context manager exits.
        """
        self.csproj = csproj
        self.temp_version = temp_version

        assert isinstance(csproj, Path)
        assert isinstance(temp_version, str)
        assert csproj.exists()

        with open(csproj, "r") as f:
            contents = f.read()

        self.before_version = re.search(
            r"<AssemblyVersion>(.*)</AssemblyVersion>", contents
        ).group(1)

    def __enter__(self) -> None:
        with open(self.csproj, "r") as f:
            contents = f.read()
        contents = re.sub(
            r"<AssemblyVersion>.*</AssemblyVersion>",
            f"<AssemblyVersion>{self.temp_version}</AssemblyVersion>",
            contents,
        )
        with open(self.csproj, "w") as f:
            f.write(contents)

    def __exit__(self, *args) -> None:
        with open(self.csproj, "r") as f:
            contents = f.read()
        contents = re.sub(
            r"<AssemblyVersion>.*</AssemblyVersion>",
            f"<AssemblyVersion>{self.before_version}</AssemblyVersion>",
            contents,
        )
        with open(self.csproj, "w") as f:
            f.write(contents)


@requires_dotnet
class CreateProject(unittest.TestCase):
    def test_build_csharp_module(self):
        """
        Ensure that the CLI is able to create and compile a csharp project. This test does not depend on a running spacetimedb instance. Skips if dotnet 8.0 is not available
        """

        bindings = Path(STDB_DIR) / "crates" / "bindings-csharp"
        codegen = bindings / "Codegen"
        runtime = bindings / "Runtime"

        temp_version = "1337.666.2048"

        try:

            run_cmd("dotnet", "workload", "install", "wasi-experimental")

            # we temporarily override the version of the dependencies during the test
            # to ensure they are not fetched from NuGet
            with tempfile.TemporaryDirectory() as tmpdir, CSProjVersionOverride(
                codegen / "Codegen.csproj", temp_version
            ), CSProjVersionOverride(runtime / "Runtime.csproj", temp_version):

                run_cmd("dotnet", "pack", cwd=codegen, capture_stderr=True)
                run_cmd("dotnet", "pack", cwd=runtime, capture_stderr=True)

                spacetime("init", "--lang=csharp", tmpdir)

                codegen_bin = codegen / "bin" / "Release"
                runtime_bin = runtime / "bin" / "Release"

                csproj = Path(tmpdir) / "StdbModule.csproj"
                with open(csproj, "r") as f:
                    contents = f.read()

                contents = contents.replace(
                    "</PropertyGroup>",
                    f"""<RestoreAdditionalProjectSources>{codegen_bin.absolute()};{runtime_bin.absolute()}</RestoreAdditionalProjectSources>
</PropertyGroup>""",
                )
                contents = re.sub(
                    r"<PackageReference Include=\"SpacetimeDB.Codegen\" Version=\".*\" />",
                    f'<PackageReference Include="SpacetimeDB.Codegen" Version="{temp_version}" />',
                    contents,
                )
                contents = re.sub(
                    r"<PackageReference Include=\"SpacetimeDB.Runtime\" Version=\".*\" />",
                    f'<PackageReference Include="SpacetimeDB.Runtime" Version="{temp_version}" />',
                    contents,
                )
                with open(csproj, "w") as f:
                    f.write(contents)

                run_cmd("dotnet", "build", cwd=tmpdir, capture_stderr=True)

        except subprocess.CalledProcessError as e:
            print(e)
            print("output:")
            print(e.output)
            raise e
