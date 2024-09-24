from .. import run_cmd, STDB_DIR, requires_dotnet, spacetime
import unittest
import tempfile
from pathlib import Path
import shutil
import subprocess
import xml.etree.ElementTree as xml


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

                config = xml.Element("configuration")

                sources = xml.SubElement(config, "packageSources")
                mappings = xml.SubElement(config, "packageSourceMapping")

                for project in packed_projects:
                    # Add local build directories as NuGet repositories.
                    path = bindings / project / "bin" / "Release"
                    xml.SubElement(sources, "add", key=project, value=str(path))

                    # Add strict package source mappings to ensure that
                    # SpacetimeDB.* packages are used from those directories
                    # and never from nuget.org.
                    #
                    # This prevents bugs where we silently used an outdated
                    # version which led to tests passing when they shouldn't.
                    mapping = xml.SubElement(mappings, "packageSource", key=project)
                    xml.SubElement(mapping, "package", pattern=project)

                xml.indent(config)
                config = xml.tostring(config, encoding="unicode", xml_declaration=True)

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
