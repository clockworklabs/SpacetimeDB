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

#[spacetimedb::view(name = player, public)]
pub fn player(ctx: &ViewContext) -> Option<PlayerState> {
    ctx.db.player_state().id().find(0u64)
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

        self.assertSql("SELECT * FROM st_view_column", """\
 view_id | col_pos | col_name | col_type      
---------+---------+----------+----------
 4096    | 0       | "id"     | 0x0d
 4096    | 1       | "level"  | 0x0d
""")

class FailPublish(Smoketest):
    AUTOPUBLISH = False

    MODULE_CODE_BROKEN_NAMESPACE = """
use spacetimedb::ViewContext;

#[spacetimedb::table(name = person, public)]
pub struct Person {
    name: String,
}

#[spacetimedb::view(name = person, public)]
pub fn person(ctx: &ViewContext) -> Option<Person> {
    None
}
"""

    MODULE_CODE_BROKEN_RETURN_TYPE = """
use spacetimedb::{SpacetimeType, ViewContext};

#[derive(SpacetimeType)]
pub enum ABC {
    A,
    B,
    C,
}

#[spacetimedb::view(name = person, public)]
pub fn person(ctx: &ViewContext) -> Option<ABC> {
    None
}
"""

    def test_fail_publish_namespace_collision(self):
        """Publishing a module should fail if a table and view have the same name"""

        name = random_string()

        self.write_module_code(self.MODULE_CODE_BROKEN_NAMESPACE)

        with self.assertRaises(Exception):
            self.publish_module(name)

    def test_fail_publish_wrong_return_type(self):
        """Publishing a module should fail if the inner return type is not a product type"""

        name = random_string()

        self.write_module_code(self.MODULE_CODE_BROKEN_RETURN_TYPE)

        with self.assertRaises(Exception):
            self.publish_module(name)

class SqlViews(Smoketest):
    MODULE_CODE = """
use spacetimedb::{AnonymousViewContext, ReducerContext, Table, ViewContext};

#[derive(Copy, Clone)]
#[spacetimedb::table(name = player_state)]
#[spacetimedb::table(name = player_level)]
pub struct PlayerState {
    #[primary_key]
    id: u64,
    #[index(btree)]
    level: u64,
}

#[spacetimedb::reducer]
pub fn add_player_level(ctx: &ReducerContext, id: u64, level: u64) {
    ctx.db.player_level().insert(PlayerState { id, level });
}

#[spacetimedb::view(name = my_player_and_level, public)]
pub fn my_player_and_level(ctx: &AnonymousViewContext) -> Option<PlayerState> {
    ctx.db.player_level().id().find(0)
}

#[spacetimedb::view(name = player_and_level, public)]
pub fn player_and_level(ctx: &AnonymousViewContext) -> Vec<PlayerState> {
    ctx.db.player_level().level().filter(2u64).collect()
}

#[spacetimedb::view(name = player, public)]
pub fn player(ctx: &ViewContext) -> Option<PlayerState> {
    log::info!("player view called");
    ctx.db.player_state().id().find(42)
}

#[spacetimedb::view(name = player_none, public)]
pub fn player_none(_ctx: &ViewContext) -> Option<PlayerState> {
    None
}

#[spacetimedb::view(name = player_vec, public)]
pub fn player_vec(ctx: &ViewContext) -> Vec<PlayerState> {
    let first = ctx.db.player_state().id().find(42).unwrap();
    let second = PlayerState { id: 7, level: 3 };
    vec![first, second]
}
"""

    def assertSql(self, sql, expected):
        self.maxDiff = None
        sql_out = self.spacetime("sql", self.database_identity, sql)
        sql_out = "\n".join([line.rstrip() for line in sql_out.splitlines()])
        expected = "\n".join([line.rstrip() for line in expected.splitlines()])
        
        self.assertMultiLineEqual(sql_out, expected)

    def insert_initial_data(self):
        self.spacetime(
            "sql",
            self.database_identity,
            """\
INSERT INTO player_state (id, level) VALUES (42, 7);
""",
        )

    def call_player_view(self):

        self.assertSql("SELECT * FROM player", """\
 id | level
----+-------
 42 | 7
""")

    def test_http_sql(self):
        """This test asserts that views can be queried over HTTP SQL"""
        self.insert_initial_data()

        self.call_player_view()

        self.assertSql("SELECT * FROM player_none", """\
 id | level
----+-------
""")

        self.assertSql("SELECT * FROM player_vec", """\
 id | level
----+-------
 42 | 7
 7  | 3
""")

    # test is prefixed with 'a' to ensure it runs before any other tests,
    # since it relies on log capturing starting from an empty log.
    def test_a_view_materialization(self):
        """This test asserts whether views are materialized correctly"""
        self.insert_initial_data()
        player_called_log = "player view called"

        self.assertNotIn(player_called_log, self.logs(100))

        self.call_player_view()
        #On first call, the view is evaluated
        self.assertIn(player_called_log, self.logs(100))
    
        self.call_player_view()
        #On second call, the view is cached
        logs = self.logs(100)
        self.assertEqual(logs.count(player_called_log), 1)

        # insert to cause cache invalidation
        self.spacetime(
            "sql",
            self.database_identity,
            """\
INSERT INTO player_state (id, level) VALUES (22, 8);
""",
        )

        self.call_player_view()
        #On third call, after invalidation, the view is evaluated again
        logs = self.logs(100)
        self.assertEqual(logs.count(player_called_log), 2)

    def test_query_anonymous_view_reducer(self):
        """Tests that anonymous views are updated for reducers"""
        self.call("add_player_level", 0, 1)
        self.call("add_player_level", 1, 2)

        self.assertSql("SELECT * FROM my_player_and_level", """\
 id | level
----+-------
 0  | 1
""")

        self.assertSql("SELECT * FROM player_and_level", """\
 id | level
----+-------
 1  | 2
""")

        self.call("add_player_level", 2, 2)

        self.assertSql("SELECT * FROM player_and_level", """\
 id | level
----+-------
 1  | 2
 2  | 2
""")

        self.assertSql("SELECT * FROM player_and_level WHERE id = 2", """\
 id | level
----+-------
 2  | 2
""")
