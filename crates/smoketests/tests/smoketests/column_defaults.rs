use spacetimedb_smoketests::{
    random_string, require_dotnet, require_emscripten, require_pnpm, ModuleLanguage, Smoketest,
};

const EXPECTED_DEFAULTS: &[(&str, &str)] = &[
    ("bool_value", "true"),
    ("u8_value", "8"),
    // TODO: uncomment this and other negative values once they're fixed in Rust.
    // https://github.com/clockworklabs/SpacetimeDB/issues/5622
    //("i8_value", "-8"),
    ("u16_value", "16"),
    //("i16_value", "-16"),
    ("u32_value", "32"),
    //("i32_value", "-32"),
    // TODO: uncomment once u64s are fixed.
    // https://github.com/clockworklabs/SpacetimeDB/issues/5623
    //("u64_value", "64"),
    //("i64_value", "-64"),
    // TODO: uncomment once f32s are fixed.
    // https://github.com/clockworklabs/SpacetimeDB/issues/5624
    // https://github.com/clockworklabs/SpacetimeDB/issues/5627
    //("f32_value", "32.5"),
    //("f64_value", "-64.25"),
    // TODO: uncomment this once string default values are fixed in Rust
    //("string_value", r#""default string""#),
];

fn test_defaults(test: &mut Smoketest, publish_updated: impl FnOnce(&mut Smoketest)) {
    test.sql("INSERT INTO defaults_test_table (id) VALUES (1)").unwrap();
    publish_updated(test);

    for &(column, expected) in EXPECTED_DEFAULTS {
        let output = test
            .sql(&format!("SELECT {column} FROM defaults_test_table WHERE id = 1"))
            .unwrap();
        let actual = output.lines().last().unwrap().trim();
        assert_eq!(actual, expected, "incorrect default for column {column}");
    }
}

fn test_source_defaults(language: ModuleLanguage, project_name: &str, initial: &str, updated: &str) {
    let mut test = Smoketest::builder().autopublish(false).build();
    let database_name = format!("column-defaults-{project_name}-{}", random_string());
    let initial_project_name = format!("{project_name}-initial");
    let updated_project_name = format!("{project_name}-updated");
    test.publish()
        .name(&database_name)
        .source(language, &initial_project_name, initial)
        .run()
        .unwrap();

    test_defaults(&mut test, |test| {
        test.publish()
            .current_database()
            .unwrap()
            .break_clients(true)
            .source(language, &updated_project_name, updated)
            .run()
            .unwrap();
    });
}

#[test]
fn test_rust_column_defaults() {
    let mut test = Smoketest::builder().module_code(RUST_INITIAL).build();
    test_defaults(&mut test, |test| {
        test.write_module_code(RUST_UPDATED).unwrap();
        test.publish()
            .current_database()
            .unwrap()
            .break_clients(true)
            .run()
            .unwrap();
    });
}

#[test]
fn test_typescript_column_defaults() {
    require_pnpm!();
    test_source_defaults(
        ModuleLanguage::TypeScript,
        "column-defaults-ts",
        TYPESCRIPT_INITIAL,
        TYPESCRIPT_UPDATED,
    );
}

#[test]
fn test_csharp_column_defaults() {
    require_dotnet!();
    test_source_defaults(
        ModuleLanguage::CSharp,
        "column-defaults-csharp",
        CSHARP_INITIAL,
        CSHARP_UPDATED,
    );
}

#[test]
fn test_cpp_column_defaults() {
    require_emscripten!();
    test_source_defaults(ModuleLanguage::Cpp, "column-defaults-cpp", CPP_INITIAL, CPP_UPDATED);
}

const RUST_INITIAL: &str = r#"
#[spacetimedb::table(accessor = defaults_test_table, public)]
pub struct DefaultsTestTable {
    pub id: u32,
}
"#;

const RUST_UPDATED: &str = r#"
#[spacetimedb::table(accessor = defaults_test_table, public)]
pub struct DefaultsTestTable {
    pub id: u32,
    #[default(true)]
    pub bool_value: bool,
//    #[default(-8)]
//    pub i8_value: i8,
    #[default(8)]
    pub u8_value: u8,
//    #[default(-16)]
//    pub i16_value: i16,
    #[default(16)]
    pub u16_value: u16,
//    #[default(-32)]
//    pub i32_value: i32,
    #[default(32)]
    pub u32_value: u32,
//    #[default(-64)]
//    pub i64_value: i64,
//    #[default(64)]
//    pub u64_value: u64,
    #[default(32.5)]
    pub f32_value: f32,
//    #[default(-64.25)]
//    pub f64_value: f64,
//    #[default("default string")]
//    pub string_value: String,
}
"#;

const TYPESCRIPT_INITIAL: &str = r#"
import { schema, t, table } from "spacetimedb/server";

const defaultsTestTable = table(
  { name: "defaults_test_table", public: true },
  { id: t.u32() }
);

export default schema({ defaultsTestTable });
"#;

const TYPESCRIPT_UPDATED: &str = r#"
import { schema, t, table } from "spacetimedb/server";

const defaultsTestTable = table(
  { name: "defaults_test_table", public: true },
  {
    id: t.u32(),
    bool_value: t.bool().default(true),
    i8_value: t.i8().default(-8),
    u8_value: t.u8().default(8),
    i16_value: t.i16().default(-16),
    u16_value: t.u16().default(16),
    i32_value: t.i32().default(-32),
    u32_value: t.u32().default(32),
    i64_value: t.i64().default(-64n),
    u64_value: t.u64().default(64n),
    f32_value: t.f32().default(32.5),
    f64_value: t.f64().default(-64.25),
    string_value: t.string().default("default string"),
  }
);

export default schema({ defaultsTestTable });
"#;

const CSHARP_INITIAL: &str = r#"
using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "defaults_test_table", Public = true)]
    public partial struct DefaultsTestTable
    {
        public uint id;
    }
}
"#;

const CSHARP_UPDATED: &str = r#"
using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "defaults_test_table", Public = true)]
    public partial struct DefaultsTestTable
    {
        public uint id;
        [Default(true)] public bool bool_value;
        [Default((sbyte)-8)] public sbyte i8_value;
        [Default((byte)8)] public byte u8_value;
        [Default((short)-16)] public short i16_value;
        [Default((ushort)16)] public ushort u16_value;
        [Default(-32)] public int i32_value;
        [Default(32U)] public uint u32_value;
        [Default(-64L)] public long i64_value;
        [Default(64UL)] public ulong u64_value;
        //[Default(32.5f)] public float f32_value;
        [Default(-64.25)] public double f64_value;
        [Default("default string")] public string string_value;
    }
}
"#;

const CPP_INITIAL: &str = r#"
#include "spacetimedb.h"

using namespace SpacetimeDB;

struct DefaultsTestTable {
    uint32_t id;
};
SPACETIMEDB_STRUCT(DefaultsTestTable, id)
SPACETIMEDB_TABLE(DefaultsTestTable, defaults_test_table, Public)
"#;

const CPP_UPDATED: &str = r#"
#include "spacetimedb.h"

using namespace SpacetimeDB;

struct DefaultsTestTable {
    uint32_t id;
    bool bool_value;
    int8_t i8_value;
    uint8_t u8_value;
    int16_t i16_value;
    uint16_t u16_value;
    int32_t i32_value;
    uint32_t u32_value;
    int64_t i64_value;
    uint64_t u64_value;
    float f32_value;
    double f64_value;
    std::string string_value;
};
SPACETIMEDB_STRUCT(
    DefaultsTestTable,
    id,
    bool_value,
    i8_value,
    u8_value,
    i16_value,
    u16_value,
    i32_value,
    u32_value,
    i64_value,
    u64_value,
    f32_value,
    f64_value,
    string_value
)
SPACETIMEDB_TABLE(DefaultsTestTable, defaults_test_table, Public)
FIELD_Default(defaults_test_table, bool_value, true)
FIELD_Default(defaults_test_table, i8_value, int8_t(-8))
FIELD_Default(defaults_test_table, u8_value, uint8_t(8))
FIELD_Default(defaults_test_table, i16_value, int16_t(-16))
FIELD_Default(defaults_test_table, u16_value, uint16_t(16))
FIELD_Default(defaults_test_table, i32_value, int32_t(-32))
FIELD_Default(defaults_test_table, u32_value, uint32_t(32))
FIELD_Default(defaults_test_table, i64_value, int64_t(-64))
FIELD_Default(defaults_test_table, u64_value, uint64_t(64))
FIELD_Default(defaults_test_table, f32_value, float(32.5))
FIELD_Default(defaults_test_table, f64_value, double(-64.25))
FIELD_Default(defaults_test_table, string_value, std::string("default string"))
"#;
