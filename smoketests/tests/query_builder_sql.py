import json
import tempfile
from pathlib import Path

from .. import (
    STDB_DIR,
    Smoketest,
    parse_sql_result,
    random_string,
    requires_dotnet,
    run_cmd,
)


@requires_dotnet
class QueryBuilderSql(Smoketest):
    AUTOPUBLISH = False

    @classmethod
    def setUpClass(cls):
        cls.project_path = STDB_DIR / "crates/bindings-csharp/Codegen.Tests/fixtures/server"
        cls._temp_dir = Path(cls.enterClassContext(tempfile.TemporaryDirectory()))
        cls.config_path = cls._temp_dir / "config.toml"
        cls.reset_config()

    def test_query_builder_sql_matches_fixture(self):
        module_name = random_string(12)

        run_cmd(
            "dotnet",
            "publish",
            "-c",
            "Release",
            "/p:TargetArchitecture=wasm",
            "/p:TargetOS=wasm",
            "/p:WasmBuildNative=false",
            cwd=self.project_path,
            capture_stderr=True,
        )

        self.publish_module(module_name, clear=True, capture_stderr=True)

        self.call("Reducers.ClearGeneratedSql")
        self.call("Reducers.SeedDeterministicData")
        self.call("Reducers.GenerateSql", "basic_where")

        sql_output = self.sql(
            "SELECT Label, SqlText, ResultJson FROM GeneratedSql ORDER BY Id"
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
