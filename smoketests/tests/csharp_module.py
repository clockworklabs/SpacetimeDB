from .. import (
    STDB_CONFIG,
    STDB_DIR,
    parse_sql_result,
    random_string,
    requires_dotnet,
    run_cmd,
    spacetime,
)
import json
import re
import shutil
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as xml
from pathlib import Path


@requires_dotnet
class CreateProject(unittest.TestCase):
    def test_build_csharp_module(self):
        """
        Ensure that the CLI is able to create and compile a csharp project. This test does not depend on a running spacetimedb instance. Skips if dotnet 8.0 is not available
        """

        bindings = Path(STDB_DIR) / "crates" / "bindings-csharp"

        try:

            run_cmd("dotnet", "nuget", "locals", "all", "--clear", cwd=bindings, capture_stderr=True)
            run_cmd("dotnet", "workload", "install", "wasi-experimental", "--skip-manifest-update", cwd=STDB_DIR / "modules")
            run_cmd("dotnet", "pack", cwd=bindings, capture_stderr=True)

            with tempfile.TemporaryDirectory() as tmpdir:
                spacetime(
                    "init",
                    "--non-interactive",
                    "--lang=csharp",
                    "--project-path",
                    tmpdir,
                    "csharp-project",
                )

                server_path = Path(tmpdir) / "spacetimedb"

                packed_projects = ["BSATN.Runtime", "Runtime"]

                config = xml.Element("configuration")

                sources = xml.SubElement(config, "packageSources")
                mappings = xml.SubElement(config, "packageSourceMapping")

                def add_mapping(source, pattern):
                    mapping = xml.SubElement(mappings, "packageSource", key=source)
                    xml.SubElement(mapping, "package", pattern=pattern)

                for project in packed_projects:
                    # Add local build directories as NuGet repositories.
                    path = bindings / project / "bin" / "Release"
                    project = f"SpacetimeDB.{project}"
                    xml.SubElement(sources, "add", key=project, value=str(path))

                    # Add strict package source mappings to ensure that
                    # SpacetimeDB.* packages are used from those directories
                    # and never from nuget.org.
                    #
                    # This prevents bugs where we silently used an outdated
                    # version which led to tests passing when they shouldn't.
                    add_mapping(project, project)

                # Add fallback for other packages.
                add_mapping("nuget.org", "*")

                xml.indent(config)
                config = xml.tostring(config, encoding="unicode", xml_declaration=True)

                print("Writing `nuget.config` contents:")
                print(config)

                config_path = server_path / "nuget.config"
                with open(config_path, "w") as f:
                    f.write(config)

                run_cmd("dotnet", "publish", cwd=server_path, capture_stderr=True)

                # Validate typed query builder
                fixture_path = STDB_DIR / "crates/bindings-csharp/Codegen.Tests/fixtures/server"
                module_name = random_string(12)

                if not STDB_CONFIG:
                    self.fail("smoketest config not initialized; rerun via smoketests.__main__")

                with tempfile.TemporaryDirectory() as config_dir:
                    config_path = Path(config_dir) / "config.toml"
                    config_path.write_text(STDB_CONFIG)

                    publish_output = spacetime(
                        "--config-path",
                        str(config_path),
                        "publish",
                        module_name,
                        "-c",
                        "--project-path",
                        fixture_path,
                        "--yes",
                        capture_stderr=True,
                    )
                    identity_match = re.search(r"identity: ([0-9a-fA-F]+)", publish_output)
                    self.assertIsNotNone(identity_match, "failed to parse identity from publish output")
                    identity = identity_match.group(1)

                    def call(reducer, *args):
                        spacetime(
                            "--config-path",
                            str(config_path),
                            "call",
                            "--",
                            identity,
                            reducer,
                            *map(json.dumps, args),
                            capture_stderr=True,
                        )

                    call("Reducers.ClearGeneratedSql")
                    call("Reducers.SeedDeterministicData")
                    call("Reducers.GenerateSql", "basic_where")

                    sql_output = spacetime(
                        "--config-path",
                        str(config_path),
                        "sql",
                        "--anonymous",
                        "--",
                        identity,
                        "SELECT Label, SqlText, ResultJson FROM GeneratedSql ORDER BY Id",
                        capture_stderr=True,
                    )

                    rows = parse_sql_result(sql_output)
                    self.assertEqual(len(rows), 1)
                    row = rows[0]
                    self.assertEqual(row["Label"], "basic_where")
                    expected_sql = 'SELECT * FROM "PublicTable" WHERE ("PublicTable"."Id" = 0)'
                    self.assertEqual(row["SqlText"], expected_sql)

                    data = json.loads(row["ResultJson"])
                    self.assertEqual(len(data), 1)
                    first = data[0]
                    self.assertEqual(first["Id"], "0")
                    self.assertEqual(first["StringField"], '"Alpha"')

        except subprocess.CalledProcessError as e:
            print(e)
            print("output:")
            print(e.output)
            raise e
