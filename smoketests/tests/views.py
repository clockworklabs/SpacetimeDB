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


#[derive(Clone)]
#[spacetimedb::table(name = player_info, index(name=age_level_index, btree(columns = [age, level])))]
pub struct PlayerInfo {
    #[primary_key]
    id: u64,
    age: u64,
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

#[spacetimedb::view(name = player_info_multi_index, public)]
pub fn player_info_view(ctx: &ViewContext) -> Option<PlayerInfo> {

    log::info!("player_info called");
    ctx.db.player_info().age_level_index().filter((25u64, 7u64)).next()
    
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

        player_called_log = "player view called"
        
        # call view, with no data
        self.assertSql("SELECT * FROM player", """\
 id | level
----+-------
""")
        logs = self.logs(100)
        self.assertEqual(logs.count(player_called_log), 1)

        self.insert_initial_data()

        # Should invoke view as data is inserted
        self.call_player_view()

        logs = self.logs(100)
        self.assertEqual(logs.count(player_called_log), 2)
    
        self.call_player_view()
        # the view is cached
        logs = self.logs(100)
        self.assertEqual(logs.count(player_called_log), 2)

        # inserting new row should not trigger view invocation due to readsets
        self.spacetime(
            "sql",
            self.database_identity,
            """\
INSERT INTO player_state (id, level) VALUES (22, 8);
""",
        )

        self.call_player_view()
        logs = self.logs(100)
        self.assertEqual(logs.count(player_called_log), 2)

        # Updating the row that the view depends on should trigger re-evaluation
        self.spacetime(
            "sql",
            self.database_identity,
            """
UPDATE player_state SET level = 9 WHERE id = 42;
""",
        )

        # Call the view again
        self.spacetime(
            "sql",
            self.database_identity,
            """
Select * FROM player_state WHERE id = 42;
""",
        )

        # On fourth call, after updating the dependent row, the view is re-evaluated
        logs = self.logs(100)
        self.assertEqual(logs.count(player_called_log), 3)


        # Updating it back for other tests to work
        self.spacetime(
            "sql",
            self.database_identity,
            """
UPDATE player_state SET level = 7 WHERE id = 42;
""",
        )

    def test_view_multi_index_materialization(self):
        """This test asserts whether views using multi-column indexes are materialized correctly"""

        player_called_log = "player_info called"
        
        # call view, with no data
        self.assertSql("SELECT * FROM player_info_multi_index", """\
 id | age | level
----+-----+-------
""")

        logs = self.logs(100)
        self.assertEqual(logs.count(player_called_log), 1)

        # Insert data
        self.spacetime(
            "sql",
            self.database_identity,
            """\
INSERT INTO player_info (id, age, level) VALUES (1, 25, 7);
""",
        )

        # Should invoke view as data is inserted
        self.assertSql("SELECT * FROM player_info_multi_index", """\
 id | age | level
----+-----+-------
 1  | 25  | 7
""")
        logs = self.logs(100)
        self.assertEqual(logs.count(player_called_log), 2)


        # Inserting a row that does not match should not trigger re-evaluation
        self.spacetime(
            "sql",
            self.database_identity,
            """\
INSERT INTO player_info (id, age, level) VALUES (2, 30, 8);
""",
        )

        logs = self.logs(100)
        self.assertEqual(logs.count(player_called_log), 2)

        # Updating the row that the view depends on should trigger re-evaluation
        self.spacetime(
            "sql",
            self.database_identity,
            """
UPDATE player_info SET age = 26 WHERE id = 1;
""",
        )
        logs = self.logs(100)
        self.assertEqual(logs.count(player_called_log), 3)
        self.assertSql("SELECT * FROM player_info_multi_index", """\
 id | age | level
----+-----+-------
""")


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


class AutoMigrateViews(Smoketest):
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
    ctx.db.player_state().id().find(1u64)
}
"""

    MODULE_CODE_UPDATED = """
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
    ctx.db.player_state().id().find(2u64)
}
"""

    def assertSql(self, sql, expected):
        self.maxDiff = None
        sql_out = self.spacetime("sql", self.database_identity, sql)
        sql_out = "\n".join([line.rstrip() for line in sql_out.splitlines()])
        expected = "\n".join([line.rstrip() for line in expected.splitlines()])
        self.assertMultiLineEqual(sql_out, expected)

    def test_views_auto_migration(self):
        """Assert that views are auto-migrated correctly"""

        self.spacetime(
            "sql",
            self.database_identity,
            "INSERT INTO player_state (id, level) VALUES (1, 1);",
        )
        self.spacetime(
            "sql",
            self.database_identity,
            "INSERT INTO player_state (id, level) VALUES (2, 2);",
        )

        self.assertSql("SELECT * FROM player", """\
 id | level
----+-------
 1  | 1
""")

        self.write_module_code(self.MODULE_CODE_UPDATED)
        self.publish_module(self.database_identity, clear=False)

        self.assertSql("SELECT * FROM player", """\
 id | level
----+-------
 2  | 2
""")


class AutoMigrateViewsTrapped(Smoketest):
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
    ctx.db.player_state().id().find(1u64)
}
"""

    TRAPPED_MODULE_CODE_UPDATED = """
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
pub fn player(_ctx: &ViewContext) -> Option<PlayerState> {
    panic!("This view is trapped")
}
"""

    MODULE_CODE_RECOVERED = """
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
    ctx.db.player_state().id().find(2u64)
}
"""

    def assertSql(self, sql, expected):
        self.maxDiff = None
        sql_out = self.spacetime("sql", self.database_identity, sql)
        sql_out = "\n".join([line.rstrip() for line in sql_out.splitlines()])
        expected = "\n".join([line.rstrip() for line in expected.splitlines()])
        self.assertMultiLineEqual(sql_out, expected)

    def test_recovery_from_trapped_views_auto_migration(self):
        """Assert that view auto-migration recovers correctly after trapped migration"""

        self.spacetime(
            "sql",
            self.database_identity,
            "INSERT INTO player_state (id, level) VALUES (1, 1);",
        )

        # Trigger initial materialization
        self.assertSql("SELECT * FROM player", """\
 id | level
----+-------
 1  | 1
""")

        # Attempt to publish trapped module (should fail)
        self.write_module_code(self.TRAPPED_MODULE_CODE_UPDATED)
        with self.assertRaises(Exception):
            self.publish_module(self.database_identity, clear=False)

        # Ensure old module still serves queries
        self.assertSql("SELECT * FROM player", """\
 id | level
----+-------
 1  | 1
""")

        # Fix the module and publish again
        self.write_module_code(self.MODULE_CODE_RECOVERED)
        self.publish_module(self.database_identity, clear=False)

        self.assertSql("SELECT * FROM player", """\
 id | level
----+-------
""")

class SubscribeViews(Smoketest):
    MODULE_CODE = """
use spacetimedb::{Identity, ReducerContext, Table, ViewContext};

#[spacetimedb::table(name = player_state)]
pub struct PlayerState {
    #[primary_key]
    identity: Identity,
    #[unique]
    name: String,
}

#[spacetimedb::view(name = my_player, public)]
pub fn my_player(ctx: &ViewContext) -> Option<PlayerState> {
    ctx.db.player_state().identity().find(ctx.sender)
}

#[spacetimedb::reducer]
pub fn insert_player(ctx: &ReducerContext, name: String) {
    ctx.db.player_state().insert(PlayerState { name, identity: ctx.sender });
}
"""

    def _test_subscribing_with_different_identities(self):
        """Tests different clients subscribing to a client-specific view"""

        # Insert an identity for Alice
        self.call("insert_player", "Alice")

        # Generate and insert a new identity for Bob
        self.reset_config()
        self.new_identity()
        self.call("insert_player", "Bob")

        # Subscribe to `my_player` as Bob
        sub = self.subscribe("select * from my_player", n=0)
        events = sub()

        # Project out the identity field.
        # TODO: Eventually we should be able to do this directly in the sql.
        # But for now we implement it in python.
        projection = [
            {
                'my_player': {
                    'deletes': [
                        {'name': row['name']}
                        for row in event['my_player']['deletes']
                    ],
                    'inserts': [
                        {'name': row['name']}
                        for row in event['my_player']['inserts']
                    ],
                }
            }
            for event in events
        ]

        self.assertEqual(
            [
                {
                    'my_player': {
                        'deletes': [],
                        'inserts': [{'name': 'Bob'}],
                    }
                },
            ],
            projection,
        )
