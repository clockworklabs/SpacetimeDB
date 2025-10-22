from .. import Smoketest, random_string


class Views(Smoketest):
    MODULE_CODE = """
use spacetimedb::ViewContext;

#[derive(Copy, Clone)]
#[spacetimedb::table(name = player_state)]
pub struct PlayerState {
    #[primary_key]
    id: u64,
    #[index(btree)]
    level: u64,
}

#[spacetimedb::view(public)]
pub fn player(ctx: &ViewContext, id: u64) -> Option<PlayerState> {
    ctx.db.player_state().id().find(id)
}
"""

    def assertSql(self, sql, expected):
        self.maxDiff = None
        sql_out = self.spacetime("sql", self.database_identity, sql)
        sql_out = "\n".join([line.rstrip() for line in sql_out.splitlines()])
        expected = "\n".join([line.rstrip() for line in expected.splitlines()])
        self.assertMultiLineEqual(sql_out, expected)

    def test_st_view_tables(self):
        """This test asserts that views populate the st_view_* system tables"""

        self.assertSql("SELECT * FROM st_view", """\
 view_id | view_name | table_id      | is_public | is_anonymous          
---------+-----------+---------------+-----------+--------------
 4096    | "player"  | (some = 4097) | true      | false
""")
        
        self.assertSql("SELECT * FROM st_view_param", """\
 view_id | param_pos | param_name | param_type      
---------+-----------+------------+------------
 4096    | 0         | "id"       | 0x0d
""")
        
        self.assertSql("SELECT * FROM st_view_column", """\
 view_id | col_pos | col_name | col_type      
---------+---------+----------+----------
 4096    | 0       | "id"     | 0x0d
 4096    | 1       | "level"  | 0x0d
""")

class FailPublish(Smoketest):
    AUTOPUBLISH = False

    MODULE_CODE_BROKEN = """
use spacetimedb::ViewContext;

#[spacetimedb::table(name = person, public)]
pub struct Person {
    name: String,
}

#[spacetimedb::view(public)]
pub fn person(ctx: &ViewContext) -> Option<Person> {
    None
}
"""

    def test_fail_publish(self):
        """This tests server side view validation on publish"""

        name = random_string()

        self.write_module_code(self.MODULE_CODE_BROKEN)

        with self.assertRaises(Exception):
            self.publish_module(name)
