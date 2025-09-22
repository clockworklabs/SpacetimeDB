from .. import Smoketest
import subprocess
import os
import tomllib
import psycopg2


def psql(identity: str, sql: str) -> str:
    """Call `psql` and execute the given SQL statement."""
    result = subprocess.run(
        ["psql", "-h", "127.0.0.1", "-p", "5432", "-U", "postgres", "-d", "quickstart", "--quiet", "-c", sql],
        encoding="utf8",
        env={**os.environ, "PGPASSWORD": identity},
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    if result.stderr:
        raise Exception(result.stderr.strip())
    return result.stdout.strip()


def connect_db(identity: str):
    """Connect to the database using `psycopg2`."""
    conn = psycopg2.connect(host="127.0.0.1", port=5432, user="postgres", password=identity, dbname="quickstart")
    conn.set_session(autocommit=True)  # Disable automic transaction
    return conn


class SqlFormat(Smoketest):
    AUTOPUBLISH = False
    MODULE_CODE = """
use spacetimedb::sats::{i256, u256};
use spacetimedb::{ConnectionId, Identity, ReducerContext, SpacetimeType, Table, Timestamp, TimeDuration};

#[derive(Copy, Clone)]
#[spacetimedb::table(name = t_ints, public)]
pub struct TInts {
    i8: i8,
    i16: i16,
    i32: i32,
    i64: i64,
    i128: i128,
    i256: i256,
}

#[spacetimedb::table(name = t_ints_tuple, public)]
pub struct TIntsTuple {
    tuple: TInts,
}

#[derive(Copy, Clone)]
#[spacetimedb::table(name = t_uints, public)]
pub struct TUints {
    u8: u8,
    u16: u16,
    u32: u32,
    u64: u64,
    u128: u128,
    u256: u256,
}

#[spacetimedb::table(name = t_uints_tuple, public)]
pub struct TUintsTuple {
    tuple: TUints,
}

#[derive(Clone)]
#[spacetimedb::table(name = t_others, public)]
pub struct TOthers {
    bool: bool,
    f32: f32,
    f64: f64,
    str: String,
    bytes: Vec<u8>,
    identity: Identity,
    connection_id: ConnectionId,
    timestamp: Timestamp,
    duration:  TimeDuration,
}

#[spacetimedb::table(name = t_others_tuple, public)]
pub struct TOthersTuple {
    tuple: TOthers
}

#[derive(SpacetimeType, Debug, Clone, Copy)]
pub enum Action {
    Inactive,
    Active,
}

#[derive(SpacetimeType, Debug, Clone, Copy)]
pub enum Color {
    Gray(u8),
}

#[derive(Copy, Clone)]
#[spacetimedb::table(name = t_simple_enum, public)]
pub struct TSimpleEnum {
    id : u32,
    action: Action,
}

#[spacetimedb::table(name = t_enum, public)]
pub struct TEnum {
    id : u32,
    color: Color,
}

#[spacetimedb::table(name = t_nested, public)]
pub struct TNested {
   en: TEnum,
   se: TSimpleEnum,
   ints: TInts,
}

#[spacetimedb::reducer]
pub fn test(ctx: &ReducerContext) {
    let tuple = TInts {
        i8: -25,
        i16: -3224,
        i32: -23443,
        i64: -2344353,
        i128: -234434897853,
        i256: (-234434897853i128).into(),
    };
    let ints = tuple;
    ctx.db.t_ints().insert(tuple);
    ctx.db.t_ints_tuple().insert(TIntsTuple { tuple });

    let tuple = TUints {
        u8: 105,
        u16: 1050,
        u32: 83892,
        u64: 48937498,
        u128: 4378528978889,
        u256: 4378528978889u128.into(),
    };
    ctx.db.t_uints().insert(tuple);
    ctx.db.t_uints_tuple().insert(TUintsTuple { tuple });

    let tuple = TOthers {
        bool: true,
        f32: 594806.58906,
        f64: -3454353.345389043278459,
        str: "This is spacetimedb".to_string(),
        bytes: vec!(1, 2, 3, 4, 5, 6, 7),
        identity: Identity::ONE,
        connection_id: ConnectionId::ZERO,      
        timestamp: Timestamp::UNIX_EPOCH,
        duration: TimeDuration::from_micros(1000 * 10000),
    };
    ctx.db.t_others().insert(tuple.clone());
    ctx.db.t_others_tuple().insert(TOthersTuple { tuple });
    
    ctx.db.t_simple_enum().insert(TSimpleEnum { id: 1, action: Action::Inactive });
    ctx.db.t_simple_enum().insert(TSimpleEnum { id: 2, action: Action::Active });
    
    ctx.db.t_enum().insert(TEnum { id: 1, color: Color::Gray(128) });
    
    ctx.db.t_nested().insert(TNested {
        en: TEnum { id: 1, color: Color::Gray(128) },
        se: TSimpleEnum { id: 2, action: Action::Active },
        ints,
    });
}
"""

    def assertSql(self, token: str, sql: str, expected):
        self.maxDiff = None
        sql_out = psql(token, sql)
        sql_out = "\n".join([line.rstrip() for line in sql_out.splitlines()])
        expected = "\n".join([line.rstrip() for line in expected.splitlines()])
        print(sql_out)
        self.assertMultiLineEqual(sql_out, expected)

    def read_token(self):
        """Read the token from the config file."""
        with open(self.config_path, "rb") as f:
            config = tomllib.load(f)
            return config['spacetimedb_token']

    def test_sql_format(self):
        """This test is designed to test calling `psql` to execute SQL statements"""
        token = self.read_token()
        self.publish_module("quickstart", clear=True)

        self.call("test")

        self.assertSql(token, "SELECT * FROM t_ints", """\
i8  |  i16  |  i32   |   i64    |     i128      |     i256
-----+-------+--------+----------+---------------+---------------
 -25 | -3224 | -23443 | -2344353 | -234434897853 | -234434897853
(1 row)""")
        self.assertSql(token, "SELECT * FROM t_ints_tuple", """\
tuple
---------------------------------------------------------------------------------------------------------
 {"i8": -25, "i16": -3224, "i32": -23443, "i64": -2344353, "i128": -234434897853, "i256": -234434897853}
(1 row)""")
        self.assertSql(token, "SELECT * FROM t_uints", """\
u8  | u16  |  u32  |   u64    |     u128      |     u256
-----+------+-------+----------+---------------+---------------
 105 | 1050 | 83892 | 48937498 | 4378528978889 | 4378528978889
(1 row)""")
        self.assertSql(token, "SELECT * FROM t_uints_tuple", """\
tuple
-------------------------------------------------------------------------------------------------------
 {"u8": 105, "u16": 1050, "u32": 83892, "u64": 48937498, "u128": 4378528978889, "u256": 4378528978889}
(1 row)""")
        self.assertSql(token, "SELECT * FROM t_others", """\
bool |    f32    |         f64         |         str         |      bytes       |                              identity                              |           connection_id            |         timestamp         | duration
------+-----------+---------------------+---------------------+------------------+--------------------------------------------------------------------+------------------------------------+---------------------------+----------
 t    | 594806.56 | -3454353.3453890434 | This is spacetimedb | \\x01020304050607 | \\x0000000000000000000000000000000000000000000000000000000000000001 | \\x00000000000000000000000000000000 | 1970-01-01T00:00:00+00:00 | PT10S
(1 row)""")
        self.assertSql(token, "SELECT * FROM t_others_tuple", """\
tuple
---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
 {"bool": true, "f32": 594806.56, "f64": -3454353.3453890434, "str": "This is spacetimedb", "bytes": "0x01020304050607", "identity": "0x0000000000000000000000000000000000000000000000000000000000000001", "connection_id": "0x00000000000000000000000000000000", "timestamp": "1970-01-01T00:00:00+00:00", "duration": "PT10S"}
(1 row)""")
        self.assertSql(token, "SELECT * FROM t_simple_enum", """\
id |  action
----+----------
  1 | Inactive
  2 | Active
(2 rows)""")
        self.assertSql(token, "SELECT * FROM t_enum", """\
id |     color
----+---------------
  1 | {"Gray": 128}
(1 row)""")
        self.assertSql(token, "SELECT * FROM t_nested", """\
en                 |                 se                  |                                                  ints
-----------------------------------+-------------------------------------+---------------------------------------------------------------------------------------------------------
 {"id": 1, "color": {"Gray": 128}} | {"id": 2, "action": {"Active": {}}} | {"i8": -25, "i16": -3224, "i32": -23443, "i64": -2344353, "i128": -234434897853, "i256": -234434897853}
(1 row)""")

    def test_sql_conn(self):
        """This test is designed to test connecting to the database and executing queries using `psycopg2`"""
        token = self.read_token()
        self.publish_module("quickstart", clear=True)
        self.call("test")

        conn = connect_db(token)
        # Check prepared statements (faked by `psycopg2`)
        with conn.cursor() as cur:
            cur.execute("select * from t_uints where u8 = %s and u16 = %s", (105, 1050))
            rows = cur.fetchall()
            self.assertEqual(rows[0], (105, 1050, 83892, 48937498, 4378528978889, 4378528978889))
        # Check long-lived connection
        with conn.cursor() as cur:
            for _ in range(10):
                cur.execute("select count(*) as t from t_uints")
                rows = cur.fetchall()
                self.assertEqual(rows[0], (1,))
        conn.close()

    def test_failures(self):
        """This test is designed to test failure cases"""
        token = self.read_token()
        self.publish_module("quickstart", clear=True)

        # Empty query
        sql_out = psql(token, "")
        self.assertEqual(sql_out, "")

        # Connection fails with invalid token
        with self.assertRaises(Exception) as cm:
            psql("invalid_token", "SELECT * FROM t_uints")
        self.assertIn("Invalid token", str(cm.exception))

        # Returns error for unsupported `sql` statements
        with self.assertRaises(Exception) as cm:
            psql(token, "SELECT CASE a WHEN 1 THEN 'one' ELSE 'other' END FROM t_uints")
        self.assertIn("Unsupported", str(cm.exception))

        # And prepared statements
        with self.assertRaises(Exception) as cm:
            psql(token, "SELECT * FROM t_uints where u8 = $1")
        self.assertIn("Unsupported", str(cm.exception))
