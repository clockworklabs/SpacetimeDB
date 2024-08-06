from .. import Smoketest

class SqlFormat(Smoketest):
    MODULE_CODE = """
use spacetimedb::{spacetimedb, sats::{i256, u256}};

#[derive(Copy, Clone)]
#[spacetimedb(table)]
pub struct TInts {
    i8: i8,
    i16: i16,
    i32: i32,
    i64: i64,
    i128: i128,
    i256: i256,
}

#[spacetimedb(table)]
pub struct TIntsTuple {
    tuple: TInts,
}

#[derive(Copy, Clone)]
#[spacetimedb(table)]
pub struct TUints {
    u8: u8,
    u16: u16,
    u32: u32,
    u64: u64,
    u128: u128,
    u256: u256,
}

#[spacetimedb(table)]
pub struct TUintsTuple {
    tuple: TUints,
}

#[derive(Clone)]
#[spacetimedb(table)]
pub struct TOthers {
    bool: bool,
    f32: f32,
    f64: f64,
    str: String,
    bytes: Vec<u8>,
}

#[spacetimedb(table)]
pub struct TOthersTuple {
    tuple: TOthers
}

#[spacetimedb(reducer)]
pub fn test() {
    let tuple = TInts {
        i8: -25,
        i16: -3224,
        i32: -23443,
        i64: -2344353,
        i128: -234434897853,
        i256: (-234434897853i128).into(),
    };
    TInts::insert(tuple);
    TIntsTuple::insert(TIntsTuple { tuple });

    let tuple = TUints {
        u8: 105,
        u16: 1050,
        u32: 83892,
        u64: 48937498,
        u128: 4378528978889,
        u256: 4378528978889u128.into(),
    };
    TUints::insert(tuple);
    TUintsTuple::insert(TUintsTuple { tuple });

    let tuple = TOthers {
        bool: true,
        f32: 594806.58906,
        f64: -3454353.345389043278459,
        str: "This is spacetimedb".to_string(),
        bytes: vec!(1, 2, 3, 4, 5, 6, 7),
    };
    TOthers::insert(tuple.clone());
    TOthersTuple::insert(TOthersTuple { tuple });
}
"""

    def assertSql(self, sql, expected):
        self.maxDiff = None
        sql_out = self.spacetime("sql", self.address, sql)
        sql_out = "\n".join([line.rstrip() for line in sql_out.splitlines()])
        expected = "\n".join([line.rstrip() for line in expected.splitlines()])
        self.assertMultiLineEqual(sql_out, expected)

    def test_sql_format(self):
        """This test is designed to test the format of the output of sql queries"""

        self.call("test")

        self.assertSql("SELECT * FROM TInts", """\
 i8  | i16   | i32    | i64      | i128          | i256          
-----+-------+--------+----------+---------------+---------------
 -25 | -3224 | -23443 | -2344353 | -234434897853 | -234434897853 
""")
        self.assertSql("SELECT * FROM TIntsTuple", """\
 tuple                                                                                             
---------------------------------------------------------------------------------------------------
 (i8 = -25, i16 = -3224, i32 = -23443, i64 = -2344353, i128 = -234434897853, i256 = -234434897853)
""")
        self.assertSql("SELECT * FROM TUints", """\
 u8  | u16  | u32   | u64      | u128          | u256          
-----+------+-------+----------+---------------+---------------
 105 | 1050 | 83892 | 48937498 | 4378528978889 | 4378528978889 
""")
        self.assertSql("SELECT * FROM TUintsTuple", """\
 tuple                                                                                           
-------------------------------------------------------------------------------------------------
 (u8 = 105, u16 = 1050, u32 = 83892, u64 = 48937498, u128 = 4378528978889, u256 = 4378528978889) 
""")
        self.assertSql("SELECT * FROM TOthers", """\
 bool | f32       | f64                | str                   | bytes            
------+-----------+--------------------+-----------------------+------------------
 true | 594806.56 | -3454353.345389043 | "This is spacetimedb" | 0x01020304050607 
""")
        self.assertSql("SELECT * FROM TOthersTuple", """\
 tuple                                                                                                           
-----------------------------------------------------------------------------------------------------------------
 (bool = true, f32 = 594806.56, f64 = -3454353.345389043, str = "This is spacetimedb", bytes = 0x01020304050607) 
""")
