#include <spacetimedb.h>
#include <variant>
#include <optional>

using namespace SpacetimeDb;

// Field accessors will be declared directly under each table


// =============================================================================
// C++ SDK Test Module - Full Equivalence with Rust and C# SDKs
// =============================================================================
//
// This module provides complete equivalence with the Rust and C# SDK test modules:
// - All primitive types, enums, structs, and vectors
// - Table operations with constraint support
// - Comprehensive reducer functionality matching other SDKs
// - Full constraint and primary key support
// - Optional types and complex nested structures
// =============================================================================

// =============================================================================
// ENUMS - Full Equivalence with C# and Rust
// =============================================================================

// SimpleEnum - C++ SDK supports basic C++ enums with U8 value!
// Using unified SPACETIMEDB_ENUM with simple syntax (auto-detects non-parenthesized arguments)
SPACETIMEDB_ENUM(SimpleEnum, Zero, One, Two)

// EnumWithPayload - Using unified SPACETIMEDB_ENUM with complex syntax!
// Auto-detects parenthesized pairs for variant enums
// Full 23 variants - now supported!
SPACETIMEDB_ENUM(EnumWithPayload,
    (U8, uint8_t),
    (U16, uint16_t),
    (U32, uint32_t),
    (U64, uint64_t),
    (U128, SpacetimeDb::u128),
    (U256, SpacetimeDb::u256),
    (I8, int8_t),
    (I16, int16_t),
    (I32, int32_t),
    (I64, int64_t),
    (I128, SpacetimeDb::i128),
    (I256, SpacetimeDb::i256),
    (Bool, bool),
    (F32, float),
    (F64, double),
    (Str, std::string),
    (Identity, SpacetimeDb::Identity),
    (ConnectionId, SpacetimeDb::ConnectionId),
    (Timestamp, SpacetimeDb::Timestamp),
    (Bytes, std::vector<uint8_t>),
    (Ints, std::vector<int32_t>),
    (Strings, std::vector<std::string>),
    (SimpleEnums, std::vector<SimpleEnum>)
)

// =============================================================================
// STRUCTS - Full Equivalence with C# and Rust  
// =============================================================================

// True unit struct - 0 fields in schema
SPACETIMEDB_UNIT_STRUCT(UnitStruct)


struct ByteStruct {
    uint8_t b;
};
SPACETIMEDB_STRUCT(ByteStruct, b)

struct EveryPrimitiveStruct {
    uint8_t a;
    uint16_t b;
    uint32_t c;
    uint64_t d;
    u128 e;
    u256 f;
    int8_t g;
    int16_t h;
    int32_t i;
    int64_t j;
    i128 k;
    i256 l;
    bool m;
    float n;
    double o;
    std::string p;
    Identity q;
    ConnectionId r;
    Timestamp s;
    TimeDuration t;
};
SPACETIMEDB_STRUCT(EveryPrimitiveStruct, a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r, s, t)

struct EveryVecStruct {
    std::vector<uint8_t> a;
    std::vector<uint16_t> b;
    std::vector<uint32_t> c;
    std::vector<uint64_t> d;
    std::vector<u128> e;
    std::vector<u256> f;
    std::vector<int8_t> g;
    std::vector<int16_t> h;
    std::vector<int32_t> i;
    std::vector<int64_t> j;
    std::vector<i128> k;
    std::vector<i256> l;
    std::vector<bool> m;
    std::vector<float> n;
    std::vector<double> o;
    std::vector<std::string> p;
    std::vector<Identity> q;
    std::vector<ConnectionId> r;
    std::vector<Timestamp> s;
    std::vector<TimeDuration> t;
};
SPACETIMEDB_STRUCT(EveryVecStruct, a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r, s, t)

// =============================================================================
// SINGLE VALUE TABLES - Matching Rust's OneXXX pattern
// =============================================================================

// Unsigned integer tables
struct OneU8 { uint8_t n; };
SPACETIMEDB_STRUCT(OneU8, n)
SPACETIMEDB_TABLE(OneU8, one_u8, Public)

struct OneU16 { uint16_t n; };
SPACETIMEDB_STRUCT(OneU16, n)
SPACETIMEDB_TABLE(OneU16, one_u16, Public)

struct OneU32 { uint32_t n; };
SPACETIMEDB_STRUCT(OneU32, n)
SPACETIMEDB_TABLE(OneU32, one_u32, Public)

struct OneU64 { uint64_t n; };
SPACETIMEDB_STRUCT(OneU64, n)
SPACETIMEDB_TABLE(OneU64, one_u64, Public)

struct OneU128 { u128 n; };
SPACETIMEDB_STRUCT(OneU128, n)
SPACETIMEDB_TABLE(OneU128, one_u128, Public)

struct OneU256 { u256 n; };
SPACETIMEDB_STRUCT(OneU256, n)
SPACETIMEDB_TABLE(OneU256, one_u256, Public)

// Signed integer tables
struct OneI8 { int8_t n; };
SPACETIMEDB_STRUCT(OneI8, n)
SPACETIMEDB_TABLE(OneI8, one_i8, Public)

struct OneI16 { int16_t n; };
SPACETIMEDB_STRUCT(OneI16, n)
SPACETIMEDB_TABLE(OneI16, one_i16, Public)

struct OneI32 { int32_t n; };
SPACETIMEDB_STRUCT(OneI32, n)
SPACETIMEDB_TABLE(OneI32, one_i32, Public)

struct OneI64 { int64_t n; };
SPACETIMEDB_STRUCT(OneI64, n)
SPACETIMEDB_TABLE(OneI64, one_i64, Public)

struct OneI128 { i128 n; };
SPACETIMEDB_STRUCT(OneI128, n)
SPACETIMEDB_TABLE(OneI128, one_i128, Public)

struct OneI256 { i256 n; };
SPACETIMEDB_STRUCT(OneI256, n)
SPACETIMEDB_TABLE(OneI256, one_i256, Public)

// Boolean and float tables
struct OneBool { bool b; };
SPACETIMEDB_STRUCT(OneBool, b)
SPACETIMEDB_TABLE(OneBool, one_bool, Public)

struct OneF32 { float f; };
SPACETIMEDB_STRUCT(OneF32, f)
SPACETIMEDB_TABLE(OneF32, one_f32, Public)

struct OneF64 { double f; };
SPACETIMEDB_STRUCT(OneF64, f)
SPACETIMEDB_TABLE(OneF64, one_f64, Public)

// String and special type tables
struct OneString { std::string s; };
SPACETIMEDB_STRUCT(OneString, s)
SPACETIMEDB_TABLE(OneString, one_string, Public)

struct OneIdentity { Identity i; };
SPACETIMEDB_STRUCT(OneIdentity, i)
SPACETIMEDB_TABLE(OneIdentity, one_identity, Public)

struct OneConnectionId { ConnectionId a; };
SPACETIMEDB_STRUCT(OneConnectionId, a)
SPACETIMEDB_TABLE(OneConnectionId, one_connection_id, Public)

struct OneTimestamp { Timestamp t; };
SPACETIMEDB_STRUCT(OneTimestamp, t)
SPACETIMEDB_TABLE(OneTimestamp, one_timestamp, Public)

// Enum and struct tables
struct OneSimpleEnum { SimpleEnum e; };
SPACETIMEDB_STRUCT(OneSimpleEnum, e)
SPACETIMEDB_TABLE(OneSimpleEnum, one_simple_enum, Public)

struct OneEnumWithPayload { EnumWithPayload e; };
SPACETIMEDB_STRUCT(OneEnumWithPayload, e)
SPACETIMEDB_TABLE(OneEnumWithPayload, one_enum_with_payload, Public)

struct OneUnitStruct { UnitStruct s; };
SPACETIMEDB_STRUCT(OneUnitStruct, s)
SPACETIMEDB_TABLE(OneUnitStruct, one_unit_struct, Public)

struct OneByteStruct { ByteStruct s; };
SPACETIMEDB_STRUCT(OneByteStruct, s)
SPACETIMEDB_TABLE(OneByteStruct, one_byte_struct, Public)

struct OneEveryPrimitiveStruct { EveryPrimitiveStruct s; };
SPACETIMEDB_STRUCT(OneEveryPrimitiveStruct, s)
SPACETIMEDB_TABLE(OneEveryPrimitiveStruct, one_every_primitive_struct, Public)

struct OneEveryVecStruct { EveryVecStruct s; };
SPACETIMEDB_STRUCT(OneEveryVecStruct, s)
SPACETIMEDB_TABLE(OneEveryVecStruct, one_every_vec_struct, Public)

// =============================================================================
// VECTOR TABLES - Matching Rust's VecXXX pattern
// =============================================================================

struct VecU8 { std::vector<uint8_t> n; };
SPACETIMEDB_STRUCT(VecU8, n)
SPACETIMEDB_TABLE(VecU8, vec_u8, Public)

struct VecU16 { std::vector<uint16_t> n; };
SPACETIMEDB_STRUCT(VecU16, n)
SPACETIMEDB_TABLE(VecU16, vec_u16, Public)

struct VecU32 { std::vector<uint32_t> n; };
SPACETIMEDB_STRUCT(VecU32, n)
SPACETIMEDB_TABLE(VecU32, vec_u32, Public)

struct VecU64 { std::vector<uint64_t> n; };
SPACETIMEDB_STRUCT(VecU64, n)
SPACETIMEDB_TABLE(VecU64, vec_u64, Public)

struct VecU128 { std::vector<u128> n; };
SPACETIMEDB_STRUCT(VecU128, n)
SPACETIMEDB_TABLE(VecU128, vec_u128, Public)

struct VecU256 { std::vector<u256> n; };
SPACETIMEDB_STRUCT(VecU256, n)
SPACETIMEDB_TABLE(VecU256, vec_u256, Public)

struct VecI8 { std::vector<int8_t> n; };
SPACETIMEDB_STRUCT(VecI8, n)
SPACETIMEDB_TABLE(VecI8, vec_i8, Public)

struct VecI16 { std::vector<int16_t> n; };
SPACETIMEDB_STRUCT(VecI16, n)
SPACETIMEDB_TABLE(VecI16, vec_i16, Public)

struct VecI32 { std::vector<int32_t> n; };
SPACETIMEDB_STRUCT(VecI32, n)
SPACETIMEDB_TABLE(VecI32, vec_i32, Public)

struct VecI64 { std::vector<int64_t> n; };
SPACETIMEDB_STRUCT(VecI64, n)
SPACETIMEDB_TABLE(VecI64, vec_i64, Public)

struct VecI128 { std::vector<i128> n; };
SPACETIMEDB_STRUCT(VecI128, n)
SPACETIMEDB_TABLE(VecI128, vec_i128, Public)

struct VecI256 { std::vector<i256> n; };
SPACETIMEDB_STRUCT(VecI256, n)
SPACETIMEDB_TABLE(VecI256, vec_i256, Public)

struct VecBool { std::vector<bool> b; };
SPACETIMEDB_STRUCT(VecBool, b)
SPACETIMEDB_TABLE(VecBool, vec_bool, Public)

struct VecF32 { std::vector<float> f; };
SPACETIMEDB_STRUCT(VecF32, f)
SPACETIMEDB_TABLE(VecF32, vec_f32, Public)

struct VecF64 { std::vector<double> f; };
SPACETIMEDB_STRUCT(VecF64, f)
SPACETIMEDB_TABLE(VecF64, vec_f64, Public)

struct VecString { std::vector<std::string> s; };
SPACETIMEDB_STRUCT(VecString, s)
SPACETIMEDB_TABLE(VecString, vec_string, Public)

struct VecIdentity { std::vector<Identity> i; };
SPACETIMEDB_STRUCT(VecIdentity, i)
SPACETIMEDB_TABLE(VecIdentity, vec_identity, Public)

struct VecConnectionId { std::vector<ConnectionId> a; };
SPACETIMEDB_STRUCT(VecConnectionId, a)
SPACETIMEDB_TABLE(VecConnectionId, vec_connection_id, Public)

struct VecTimestamp { std::vector<Timestamp> t; };
SPACETIMEDB_STRUCT(VecTimestamp, t)
SPACETIMEDB_TABLE(VecTimestamp, vec_timestamp, Public)

struct VecSimpleEnum { std::vector<SimpleEnum> e; };
SPACETIMEDB_STRUCT(VecSimpleEnum, e)
SPACETIMEDB_TABLE(VecSimpleEnum, vec_simple_enum, Public)

struct VecEnumWithPayload { std::vector<EnumWithPayload> e; };
SPACETIMEDB_STRUCT(VecEnumWithPayload, e)
SPACETIMEDB_TABLE(VecEnumWithPayload, vec_enum_with_payload, Public)

struct VecUnitStruct { std::vector<UnitStruct> s; };
SPACETIMEDB_STRUCT(VecUnitStruct, s)
SPACETIMEDB_TABLE(VecUnitStruct, vec_unit_struct, Public)

struct VecByteStruct { std::vector<ByteStruct> s; };
SPACETIMEDB_STRUCT(VecByteStruct, s)
SPACETIMEDB_TABLE(VecByteStruct, vec_byte_struct, Public)

struct VecEveryPrimitiveStruct { std::vector<EveryPrimitiveStruct> s; };
SPACETIMEDB_STRUCT(VecEveryPrimitiveStruct, s)
SPACETIMEDB_TABLE(VecEveryPrimitiveStruct, vec_every_primitive_struct, Public)

struct VecEveryVecStruct { std::vector<EveryVecStruct> s; };
SPACETIMEDB_STRUCT(VecEveryVecStruct, s)
SPACETIMEDB_TABLE(VecEveryVecStruct, vec_every_vec_struct, Public)

// =============================================================================
// OPTIONAL TABLES - Using inlined std::optional instead of wrapper structs
// =============================================================================

struct OptionI32 { std::optional<int32_t> n; };
SPACETIMEDB_STRUCT(OptionI32, n)
SPACETIMEDB_TABLE(OptionI32, option_i32, Public)

struct OptionString { std::optional<std::string> s; };
SPACETIMEDB_STRUCT(OptionString, s)
SPACETIMEDB_TABLE(OptionString, option_string, Public)

struct OptionIdentity { std::optional<Identity> i; };
SPACETIMEDB_STRUCT(OptionIdentity, i)
SPACETIMEDB_TABLE(OptionIdentity, option_identity, Public)

struct OptionSimpleEnum { std::optional<SimpleEnum> e; };
SPACETIMEDB_STRUCT(OptionSimpleEnum, e)
SPACETIMEDB_TABLE(OptionSimpleEnum, option_simple_enum, Public)

struct OptionEveryPrimitiveStruct { std::optional<EveryPrimitiveStruct> s; };
SPACETIMEDB_STRUCT(OptionEveryPrimitiveStruct, s)
SPACETIMEDB_TABLE(OptionEveryPrimitiveStruct, option_every_primitive_struct, Public)

// Complex nested optional type - NOW FIXED WITH TYPE REGISTRY!
struct OptionVecOptionI32 { std::optional<std::vector<std::optional<int32_t>>> v; };
SPACETIMEDB_STRUCT(OptionVecOptionI32, v)
SPACETIMEDB_TABLE(OptionVecOptionI32, option_vec_option_i32, Public)

// =============================================================================
// UNIQUE CONSTRAINT TABLES - Matching Rust's UniqueXXX pattern
// =============================================================================

struct UniqueU8 { uint8_t n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueU8, n, data)
SPACETIMEDB_TABLE(UniqueU8, unique_u8, Public)
FIELD_Unique(unique_u8, n);

struct UniqueU16 { uint16_t n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueU16, n, data)
SPACETIMEDB_TABLE(UniqueU16, unique_u16, Public)
FIELD_Unique(unique_u16, n);

struct UniqueU32 { uint32_t n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueU32, n, data)
SPACETIMEDB_TABLE(UniqueU32, unique_u32, Public)
FIELD_Unique(unique_u32, n);

struct UniqueU64 { uint64_t n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueU64, n, data)
SPACETIMEDB_TABLE(UniqueU64, unique_u64, Public)
FIELD_Unique(unique_u64, n);

struct UniqueU128 { u128 n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueU128, n, data)
SPACETIMEDB_TABLE(UniqueU128, unique_u128, Public)
FIELD_Unique(unique_u128, n);

struct UniqueU256 { u256 n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueU256, n, data)
SPACETIMEDB_TABLE(UniqueU256, unique_u256, Public)
FIELD_Unique(unique_u256, n);

struct UniqueI8 { int8_t n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueI8, n, data)
SPACETIMEDB_TABLE(UniqueI8, unique_i8, Public)
FIELD_Unique(unique_i8, n);

struct UniqueI16 { int16_t n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueI16, n, data)
SPACETIMEDB_TABLE(UniqueI16, unique_i16, Public)
FIELD_Unique(unique_i16, n);

struct UniqueI32 { int32_t n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueI32, n, data)
SPACETIMEDB_TABLE(UniqueI32, unique_i32, Public)
FIELD_Unique(unique_i32, n);

struct UniqueI64 { int64_t n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueI64, n, data)
SPACETIMEDB_TABLE(UniqueI64, unique_i64, Public)
FIELD_Unique(unique_i64, n);

struct UniqueI128 { i128 n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueI128, n, data)
SPACETIMEDB_TABLE(UniqueI128, unique_i128, Public)
FIELD_Unique(unique_i128, n);

struct UniqueI256 { i256 n; int32_t data; };
SPACETIMEDB_STRUCT(UniqueI256, n, data)
SPACETIMEDB_TABLE(UniqueI256, unique_i256, Public)
FIELD_Unique(unique_i256, n);

struct UniqueBool { bool b; int32_t data; };
SPACETIMEDB_STRUCT(UniqueBool, b, data)
SPACETIMEDB_TABLE(UniqueBool, unique_bool, Public)
FIELD_Unique(unique_bool, b);

struct UniqueString { std::string s; int32_t data; };
SPACETIMEDB_STRUCT(UniqueString, s, data)
SPACETIMEDB_TABLE(UniqueString, unique_string, Public)
FIELD_Unique(unique_string, s);

struct UniqueIdentity { Identity i; int32_t data; };
SPACETIMEDB_STRUCT(UniqueIdentity, i, data)
SPACETIMEDB_TABLE(UniqueIdentity, unique_identity, Public)
FIELD_Unique(unique_identity, i);

struct UniqueConnectionId { ConnectionId a; int32_t data; };
SPACETIMEDB_STRUCT(UniqueConnectionId, a, data)
SPACETIMEDB_TABLE(UniqueConnectionId, unique_connection_id, Public)
FIELD_Unique(unique_connection_id, a);

// =============================================================================
// PRIMARY KEY TABLES - Matching Rust's PkXXX pattern
// =============================================================================

struct PkU8 { uint8_t n; int32_t data; };
SPACETIMEDB_STRUCT(PkU8, n, data)
SPACETIMEDB_TABLE(PkU8, pk_u8, Public)
FIELD_PrimaryKey(pk_u8, n);

struct PkU16 { uint16_t n; int32_t data; };
SPACETIMEDB_STRUCT(PkU16, n, data)
SPACETIMEDB_TABLE(PkU16, pk_u16, Public)
FIELD_PrimaryKey(pk_u16, n);

struct PkU32 { uint32_t n; int32_t data; };
SPACETIMEDB_STRUCT(PkU32, n, data)
SPACETIMEDB_TABLE(PkU32, pk_u32, Public)
FIELD_PrimaryKey(pk_u32, n);

struct PkU32Two { uint32_t n; int32_t data; };
SPACETIMEDB_STRUCT(PkU32Two, n, data)
SPACETIMEDB_TABLE(PkU32Two, pk_u32_two, Public)
FIELD_PrimaryKey(pk_u32_two, n);

struct PkU64 { uint64_t n; int32_t data; };
SPACETIMEDB_STRUCT(PkU64, n, data)
SPACETIMEDB_TABLE(PkU64, pk_u64, Public)
FIELD_PrimaryKey(pk_u64, n);

struct PkU128 { u128 n; int32_t data; };
SPACETIMEDB_STRUCT(PkU128, n, data)
SPACETIMEDB_TABLE(PkU128, pk_u128, Public)
FIELD_PrimaryKey(pk_u128, n);

struct PkU256 { u256 n; int32_t data; };
SPACETIMEDB_STRUCT(PkU256, n, data)
SPACETIMEDB_TABLE(PkU256, pk_u256, Public)
FIELD_PrimaryKey(pk_u256, n);

struct PkI8 { int8_t n; int32_t data; };
SPACETIMEDB_STRUCT(PkI8, n, data)
SPACETIMEDB_TABLE(PkI8, pk_i8, Public)
FIELD_PrimaryKey(pk_i8, n);

struct PkI16 { int16_t n; int32_t data; };
SPACETIMEDB_STRUCT(PkI16, n, data)
SPACETIMEDB_TABLE(PkI16, pk_i16, Public)
FIELD_PrimaryKey(pk_i16, n);

struct PkI32 { int32_t n; int32_t data; };
SPACETIMEDB_STRUCT(PkI32, n, data)
SPACETIMEDB_TABLE(PkI32, pk_i32, Public)
FIELD_PrimaryKey(pk_i32, n);

struct PkI64 { int64_t n; int32_t data; };
SPACETIMEDB_STRUCT(PkI64, n, data)
SPACETIMEDB_TABLE(PkI64, pk_i64, Public)
FIELD_PrimaryKey(pk_i64, n);

struct PkI128 { i128 n; int32_t data; };
SPACETIMEDB_STRUCT(PkI128, n, data)
SPACETIMEDB_TABLE(PkI128, pk_i128, Public)
FIELD_PrimaryKey(pk_i128, n);

struct PkI256 { i256 n; int32_t data; };
SPACETIMEDB_STRUCT(PkI256, n, data)
SPACETIMEDB_TABLE(PkI256, pk_i256, Public)
FIELD_PrimaryKey(pk_i256, n);

struct PkBool { bool b; int32_t data; };
SPACETIMEDB_STRUCT(PkBool, b, data)
SPACETIMEDB_TABLE(PkBool, pk_bool, Public)
FIELD_PrimaryKey(pk_bool, b);

struct PkString { std::string s; int32_t data; };
SPACETIMEDB_STRUCT(PkString, s, data)
SPACETIMEDB_TABLE(PkString, pk_string, Public)
FIELD_PrimaryKey(pk_string, s);

struct PkIdentity { Identity i; int32_t data; };
SPACETIMEDB_STRUCT(PkIdentity, i, data)
SPACETIMEDB_TABLE(PkIdentity, pk_identity, Public)
FIELD_PrimaryKey(pk_identity, i);

struct PkConnectionId { ConnectionId a; int32_t data; };
SPACETIMEDB_STRUCT(PkConnectionId, a, data)
SPACETIMEDB_TABLE(PkConnectionId, pk_connection_id, Public)
FIELD_PrimaryKey(pk_connection_id, a);

struct PkSimpleEnum { SimpleEnum a; int32_t data; };
SPACETIMEDB_STRUCT(PkSimpleEnum, a, data)
SPACETIMEDB_TABLE(PkSimpleEnum, pk_simple_enum, Public)
FIELD_PrimaryKey(pk_simple_enum, a);

// =============================================================================
// ADDITIONAL SPECIALIZED TABLES
// =============================================================================

// Large comprehensive table
struct LargeTable {
    uint8_t a;
    uint16_t b;
    uint32_t c;
    uint64_t d;
    u128 e;
    u256 f;
    int8_t g;
    int16_t h;
    int32_t i;
    int64_t j;
    i128 k;
    i256 l;
    bool m;
    float n;
    double o;
    std::string p;
    SimpleEnum q;
    EnumWithPayload r;
    UnitStruct s;
    ByteStruct t;
    EveryPrimitiveStruct u;
    EveryVecStruct v;
};
SPACETIMEDB_STRUCT(LargeTable, a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r, s, t, u, v)
SPACETIMEDB_TABLE(LargeTable, large_table, Public)

// Table that holds other table structs
struct TableHoldsTable {
    OneU8 a;
    VecU8 b;
};
SPACETIMEDB_STRUCT(TableHoldsTable, a, b)
SPACETIMEDB_TABLE(TableHoldsTable, table_holds_table, Public)

// Scheduled table
struct ScheduledTable {
    uint64_t scheduled_id;
    ScheduleAt scheduled_at;
    std::string text;
};
SPACETIMEDB_STRUCT(ScheduledTable, scheduled_id, scheduled_at, text)
SPACETIMEDB_TABLE(ScheduledTable, scheduled_table, Public)
FIELD_PrimaryKeyAutoInc(scheduled_table, scheduled_id);
SPACETIMEDB_SCHEDULE(scheduled_table, 1, send_scheduled_message)  // Column 1 is scheduled_at

// Indexed tables
struct IndexedTable {
    uint32_t player_id;
};
SPACETIMEDB_STRUCT(IndexedTable, player_id)
SPACETIMEDB_TABLE(IndexedTable, indexed_table, Private)
FIELD_Index(indexed_table, player_id);

struct IndexedTable2 {
    uint32_t player_id;
    float player_snazz;
};
SPACETIMEDB_STRUCT(IndexedTable2, player_id, player_snazz)
SPACETIMEDB_TABLE(IndexedTable2, indexed_table_2, Private)  // Remove constraint from table macro
FIELD_NamedMultiColumnIndex(indexed_table_2, player_id_snazz_index, player_id, player_snazz);

struct BTreeU32 {
    uint32_t n;
    int32_t data;
};
SPACETIMEDB_STRUCT(BTreeU32, n, data)
SPACETIMEDB_TABLE(BTreeU32, btree_u32, Public)
FIELD_Index(btree_u32, n);

struct Users {
    Identity identity;
    std::string name;
};
SPACETIMEDB_STRUCT(Users, identity, name)
SPACETIMEDB_TABLE(Users, users, Public)
FIELD_PrimaryKey(users, identity);

struct IndexedSimpleEnum {
    SimpleEnum n;
};
SPACETIMEDB_STRUCT(IndexedSimpleEnum, n)
SPACETIMEDB_TABLE(IndexedSimpleEnum, indexed_simple_enum, Public)
FIELD_Index(indexed_simple_enum, n);



// =============================================================================
// SINGLE VALUE TABLE REDUCERS - INSERT OPERATIONS
// =============================================================================

SPACETIMEDB_REDUCER(insert_one_u8, ReducerContext ctx, uint8_t n)
{
    LOG_INFO("insert_one_u8 called with value: " + std::to_string(n));
    ctx.db[one_u8].insert(OneU8{.n = n});
    LOG_INFO("insert_one_u8 completed");
}

SPACETIMEDB_REDUCER(insert_one_u16, ReducerContext ctx, uint16_t n)
{
    ctx.db[one_u16].insert(OneU16{.n = n});
}

SPACETIMEDB_REDUCER(insert_one_u32, ReducerContext ctx, uint32_t n)
{
    ctx.db[one_u32].insert(OneU32{.n = n});
}

SPACETIMEDB_REDUCER(insert_one_u64, ReducerContext ctx, uint64_t n)
{
    ctx.db[one_u64].insert(OneU64{.n = n});
}

SPACETIMEDB_REDUCER(insert_one_u128, ReducerContext ctx, u128 n)
{
    ctx.db[one_u128].insert(OneU128{.n = n});
}

SPACETIMEDB_REDUCER(insert_one_u256, ReducerContext ctx, u256 n)
{
    ctx.db[one_u256].insert(OneU256{.n = n});
}

SPACETIMEDB_REDUCER(insert_one_i8, ReducerContext ctx, int8_t n)
{
    ctx.db[one_i8].insert(OneI8{.n = n});
}

SPACETIMEDB_REDUCER(insert_one_i16, ReducerContext ctx, int16_t n)
{
    ctx.db[one_i16].insert(OneI16{.n = n});
}

SPACETIMEDB_REDUCER(insert_one_i32, ReducerContext ctx, int32_t n)
{
    ctx.db[one_i32].insert(OneI32{.n = n});
}

SPACETIMEDB_REDUCER(insert_one_i64, ReducerContext ctx, int64_t n)
{
    ctx.db[one_i64].insert(OneI64{.n = n});
}

SPACETIMEDB_REDUCER(insert_one_i128, ReducerContext ctx, i128 n)
{
    ctx.db[one_i128].insert(OneI128{.n = n});
}

SPACETIMEDB_REDUCER(insert_one_i256, ReducerContext ctx, i256 n)
{
    ctx.db[one_i256].insert(OneI256{.n = n});
}

SPACETIMEDB_REDUCER(insert_one_bool, ReducerContext ctx, bool b)
{
    ctx.db[one_bool].insert(OneBool{.b = b});
}

SPACETIMEDB_REDUCER(insert_one_f32, ReducerContext ctx, float f)
{
    ctx.db[one_f32].insert(OneF32{.f = f});
}

SPACETIMEDB_REDUCER(insert_one_f64, ReducerContext ctx, double f)
{
    ctx.db[one_f64].insert(OneF64{.f = f});
}

SPACETIMEDB_REDUCER(insert_one_string, ReducerContext ctx, std::string s)
{
    ctx.db[one_string].insert(OneString{.s = s});
}

 SPACETIMEDB_REDUCER(insert_one_identity, ReducerContext ctx, Identity i)
{
    ctx.db[one_identity].insert(OneIdentity{.i = i});
} 

 SPACETIMEDB_REDUCER(insert_one_connection_id, ReducerContext ctx, ConnectionId a)
{
    ctx.db[one_connection_id].insert(OneConnectionId{.a = a});
} 

 SPACETIMEDB_REDUCER(insert_one_timestamp, ReducerContext ctx, Timestamp t)
{
    ctx.db[one_timestamp].insert(OneTimestamp{.t = t});
} 

SPACETIMEDB_REDUCER(insert_one_simple_enum, ReducerContext ctx, SimpleEnum e)
{
    ctx.db[one_simple_enum].insert(OneSimpleEnum{.e = e});
}

SPACETIMEDB_REDUCER(insert_one_enum_with_payload, ReducerContext ctx, EnumWithPayload e)
{
    ctx.db[one_enum_with_payload].insert(OneEnumWithPayload{e});
}

SPACETIMEDB_REDUCER(insert_one_unit_struct, ReducerContext ctx, UnitStruct s)
{
    fprintf(stdout, "SUCCESS: insert_one_unit_struct reducer called with UnitStruct\n");
    ctx.db[one_unit_struct].insert(OneUnitStruct{s});
}

SPACETIMEDB_REDUCER(insert_one_byte_struct, ReducerContext ctx, ByteStruct s)
{
    ctx.db[one_byte_struct].insert(OneByteStruct{s});
}

SPACETIMEDB_REDUCER(insert_one_every_primitive_struct, ReducerContext ctx, EveryPrimitiveStruct s)
{
    ctx.db[one_every_primitive_struct].insert(OneEveryPrimitiveStruct{s});
}

 SPACETIMEDB_REDUCER(insert_one_every_vec_struct, ReducerContext ctx, EveryVecStruct s)
{
    ctx.db[one_every_vec_struct].insert(OneEveryVecStruct{s});
} 

// =============================================================================
// VECTOR TABLE REDUCERS - INSERT OPERATIONS
// =============================================================================

SPACETIMEDB_REDUCER(insert_vec_u8, ReducerContext ctx, std::vector<uint8_t> n)
{
    ctx.db[vec_u8].insert(VecU8{n});
}

SPACETIMEDB_REDUCER(insert_vec_u16, ReducerContext ctx, std::vector<uint16_t> n)
{
    ctx.db[vec_u16].insert(VecU16{n});
}

SPACETIMEDB_REDUCER(insert_vec_u32, ReducerContext ctx, std::vector<uint32_t> n)
{
    ctx.db[vec_u32].insert(VecU32{n});
}

SPACETIMEDB_REDUCER(insert_vec_u64, ReducerContext ctx, std::vector<uint64_t> n)
{
    ctx.db[vec_u64].insert(VecU64{n});
}

SPACETIMEDB_REDUCER(insert_vec_u128, ReducerContext ctx, std::vector<u128> n)
{
    ctx.db[vec_u128].insert(VecU128{n});
}

SPACETIMEDB_REDUCER(insert_vec_u256, ReducerContext ctx, std::vector<u256> n)
{
    ctx.db[vec_u256].insert(VecU256{n});
}

SPACETIMEDB_REDUCER(insert_vec_i8, ReducerContext ctx, std::vector<int8_t> n)
{
    ctx.db[vec_i8].insert(VecI8{n});
}

SPACETIMEDB_REDUCER(insert_vec_i16, ReducerContext ctx, std::vector<int16_t> n)
{
    ctx.db[vec_i16].insert(VecI16{n});
}

SPACETIMEDB_REDUCER(insert_vec_i32, ReducerContext ctx, std::vector<int32_t> n)
{
    ctx.db[vec_i32].insert(VecI32{n});
}

SPACETIMEDB_REDUCER(insert_vec_i64, ReducerContext ctx, std::vector<int64_t> n)
{
    ctx.db[vec_i64].insert(VecI64{n});
}

SPACETIMEDB_REDUCER(insert_vec_i128, ReducerContext ctx, std::vector<i128> n)
{
    ctx.db[vec_i128].insert(VecI128{n});
}

SPACETIMEDB_REDUCER(insert_vec_i256, ReducerContext ctx, std::vector<i256> n)
{
    ctx.db[vec_i256].insert(VecI256{n});
}

SPACETIMEDB_REDUCER(insert_vec_bool, ReducerContext ctx, std::vector<bool> b)
{
    ctx.db[vec_bool].insert(VecBool{b});
}

SPACETIMEDB_REDUCER(insert_vec_f32, ReducerContext ctx, std::vector<float> f)
{
    ctx.db[vec_f32].insert(VecF32{f});
}

SPACETIMEDB_REDUCER(insert_vec_f64, ReducerContext ctx, std::vector<double> f)
{
    ctx.db[vec_f64].insert(VecF64{f});
}

SPACETIMEDB_REDUCER(insert_vec_string, ReducerContext ctx, std::vector<std::string> s)
{
    ctx.db[vec_string].insert(VecString{s});
}

SPACETIMEDB_REDUCER(insert_vec_identity, ReducerContext ctx, std::vector<Identity> i)
{
    ctx.db[vec_identity].insert(VecIdentity{i});
}

SPACETIMEDB_REDUCER(insert_vec_connection_id, ReducerContext ctx, std::vector<ConnectionId> a)
{
    ctx.db[vec_connection_id].insert(VecConnectionId{a});
}

SPACETIMEDB_REDUCER(insert_vec_timestamp, ReducerContext ctx, std::vector<Timestamp> t)
{
    ctx.db[vec_timestamp].insert(VecTimestamp{t});
}

SPACETIMEDB_REDUCER(insert_vec_simple_enum, ReducerContext ctx, std::vector<SimpleEnum> e)
{
    ctx.db[vec_simple_enum].insert(VecSimpleEnum{e});
}

SPACETIMEDB_REDUCER(insert_vec_enum_with_payload, ReducerContext ctx, std::vector<EnumWithPayload> e)
{
    ctx.db[vec_enum_with_payload].insert(VecEnumWithPayload{e});
}

SPACETIMEDB_REDUCER(insert_vec_unit_struct, ReducerContext ctx, std::vector<UnitStruct> s)
{
    ctx.db[vec_unit_struct].insert(VecUnitStruct{s});
}

SPACETIMEDB_REDUCER(insert_vec_byte_struct, ReducerContext ctx, std::vector<ByteStruct> s)
{
    ctx.db[vec_byte_struct].insert(VecByteStruct{s});
}

SPACETIMEDB_REDUCER(insert_vec_every_primitive_struct, ReducerContext ctx, std::vector<EveryPrimitiveStruct> s)
{
    ctx.db[vec_every_primitive_struct].insert(VecEveryPrimitiveStruct{s});
}

 SPACETIMEDB_REDUCER(insert_vec_every_vec_struct, ReducerContext ctx, std::vector<EveryVecStruct> s)
{
    ctx.db[vec_every_vec_struct].insert(VecEveryVecStruct{s});
} 

// =============================================================================
// OPTIONAL TABLE REDUCERS - INSERT OPERATIONS
// =============================================================================

SPACETIMEDB_REDUCER(insert_option_i32, ReducerContext ctx, std::optional<int32_t> n)
{
    ctx.db[option_i32].insert(OptionI32{n});
}

SPACETIMEDB_REDUCER(insert_option_string, ReducerContext ctx, std::optional<std::string> s)
{
    ctx.db[option_string].insert(OptionString{s});
}

SPACETIMEDB_REDUCER(insert_option_identity, ReducerContext ctx, std::optional<Identity> i)
{
    ctx.db[option_identity].insert(OptionIdentity{i});
}

SPACETIMEDB_REDUCER(insert_option_simple_enum, ReducerContext ctx, std::optional<SimpleEnum> e)
{
    ctx.db[option_simple_enum].insert(OptionSimpleEnum{e});
}

SPACETIMEDB_REDUCER(insert_option_every_primitive_struct, ReducerContext ctx, std::optional<EveryPrimitiveStruct> s)
{
    ctx.db[option_every_primitive_struct].insert(OptionEveryPrimitiveStruct{s});
}

// Complex nested optional type - NOW FIXED!
 SPACETIMEDB_REDUCER(insert_option_vec_option_i32, ReducerContext ctx, std::optional<std::vector<std::optional<int32_t>>> v)
{
    ctx.db[option_vec_option_i32].insert(OptionVecOptionI32{v});
}

// =============================================================================
// UNIQUE CONSTRAINT TABLE REDUCERS
// =============================================================================

SPACETIMEDB_REDUCER(insert_unique_u8, ReducerContext ctx, uint8_t n, int32_t data)
{
    ctx.db[unique_u8].insert(UniqueU8{n, data});
}

SPACETIMEDB_REDUCER(insert_unique_u16, ReducerContext ctx, uint16_t n, int32_t data)
{
    ctx.db[unique_u16].insert(UniqueU16{n, data});
}

SPACETIMEDB_REDUCER(insert_unique_u32, ReducerContext ctx, uint32_t n, int32_t data)
{
    ctx.db[unique_u32].insert(UniqueU32{n, data});
}

SPACETIMEDB_REDUCER(insert_unique_u64, ReducerContext ctx, uint64_t n, int32_t data)
{
    ctx.db[unique_u64].insert(UniqueU64{n, data});
}

SPACETIMEDB_REDUCER(insert_unique_u128, ReducerContext ctx, u128 n, int32_t data)
{
    ctx.db[unique_u128].insert(UniqueU128{n, data});
}

SPACETIMEDB_REDUCER(insert_unique_u256, ReducerContext ctx, u256 n, int32_t data)
{
    ctx.db[unique_u256].insert(UniqueU256{n, data});
}

SPACETIMEDB_REDUCER(insert_unique_i8, ReducerContext ctx, int8_t n, int32_t data)
{
    ctx.db[unique_i8].insert(UniqueI8{n, data});
}

SPACETIMEDB_REDUCER(insert_unique_i16, ReducerContext ctx, int16_t n, int32_t data)
{
    ctx.db[unique_i16].insert(UniqueI16{n, data});
}

SPACETIMEDB_REDUCER(insert_unique_i32, ReducerContext ctx, int32_t n, int32_t data)
{
    ctx.db[unique_i32].insert(UniqueI32{n, data});
}

SPACETIMEDB_REDUCER(insert_unique_i64, ReducerContext ctx, int64_t n, int32_t data)
{
    ctx.db[unique_i64].insert(UniqueI64{n, data});
}

SPACETIMEDB_REDUCER(insert_unique_i128, ReducerContext ctx, i128 n, int32_t data)
{
    ctx.db[unique_i128].insert(UniqueI128{n, data});
}

SPACETIMEDB_REDUCER(insert_unique_i256, ReducerContext ctx, i256 n, int32_t data)
{
    ctx.db[unique_i256].insert(UniqueI256{n, data});
}

SPACETIMEDB_REDUCER(insert_unique_bool, ReducerContext ctx, bool b, int32_t data)
{
    ctx.db[unique_bool].insert(UniqueBool{b, data});
}

SPACETIMEDB_REDUCER(insert_unique_string, ReducerContext ctx, std::string s, int32_t data)
{
    ctx.db[unique_string].insert(UniqueString{s, data});
}

SPACETIMEDB_REDUCER(insert_unique_identity, ReducerContext ctx, Identity i, int32_t data)
{
    ctx.db[unique_identity].insert(UniqueIdentity{i, data});
}

SPACETIMEDB_REDUCER(insert_unique_connection_id, ReducerContext ctx, ConnectionId a, int32_t data)
{
    ctx.db[unique_connection_id].insert(UniqueConnectionId{a, data});
}

// =============================================================================
// PRIMARY KEY TABLE REDUCERS
// =============================================================================

SPACETIMEDB_REDUCER(insert_pk_u8, ReducerContext ctx, uint8_t n, int32_t data)
{
    ctx.db[pk_u8].insert(PkU8{n, data});
}

SPACETIMEDB_REDUCER(insert_pk_u16, ReducerContext ctx, uint16_t n, int32_t data)
{
    ctx.db[pk_u16].insert(PkU16{n, data});
}

SPACETIMEDB_REDUCER(insert_pk_u32, ReducerContext ctx, uint32_t n, int32_t data)
{
    ctx.db[pk_u32].insert(PkU32{n, data});
}

SPACETIMEDB_REDUCER(insert_pk_u32_two, ReducerContext ctx, uint32_t n, int32_t data)
{
    ctx.db[pk_u32_two].insert(PkU32Two{n, data});
}

SPACETIMEDB_REDUCER(insert_pk_u64, ReducerContext ctx, uint64_t n, int32_t data)
{
    ctx.db[pk_u64].insert(PkU64{n, data});
}

SPACETIMEDB_REDUCER(insert_pk_u128, ReducerContext ctx, u128 n, int32_t data)
{
    ctx.db[pk_u128].insert(PkU128{n, data});
}

SPACETIMEDB_REDUCER(insert_pk_u256, ReducerContext ctx, u256 n, int32_t data)
{
    ctx.db[pk_u256].insert(PkU256{n, data});
}

SPACETIMEDB_REDUCER(insert_pk_i8, ReducerContext ctx, int8_t n, int32_t data)
{
    ctx.db[pk_i8].insert(PkI8{n, data});
}

SPACETIMEDB_REDUCER(insert_pk_i16, ReducerContext ctx, int16_t n, int32_t data)
{
    ctx.db[pk_i16].insert(PkI16{n, data});
}

SPACETIMEDB_REDUCER(insert_pk_i32, ReducerContext ctx, int32_t n, int32_t data)
{
    ctx.db[pk_i32].insert(PkI32{n, data});
}

SPACETIMEDB_REDUCER(insert_pk_i64, ReducerContext ctx, int64_t n, int32_t data)
{
    ctx.db[pk_i64].insert(PkI64{n, data});
}

SPACETIMEDB_REDUCER(insert_pk_i128, ReducerContext ctx, i128 n, int32_t data)
{
    ctx.db[pk_i128].insert(PkI128{n, data});
}

SPACETIMEDB_REDUCER(insert_pk_i256, ReducerContext ctx, i256 n, int32_t data)
{
    ctx.db[pk_i256].insert(PkI256{n, data});
}

SPACETIMEDB_REDUCER(insert_pk_bool, ReducerContext ctx, bool b, int32_t data)
{
    ctx.db[pk_bool].insert(PkBool{b, data});
}

SPACETIMEDB_REDUCER(insert_pk_string, ReducerContext ctx, std::string s, int32_t data)
{
    ctx.db[pk_string].insert(PkString{s, data});
}

SPACETIMEDB_REDUCER(insert_pk_identity, ReducerContext ctx, Identity i, int32_t data)
{
    ctx.db[pk_identity].insert(PkIdentity{i, data});
}

SPACETIMEDB_REDUCER(insert_pk_connection_id, ReducerContext ctx, ConnectionId a, int32_t data)
{
    ctx.db[pk_connection_id].insert(PkConnectionId{a, data});
}

SPACETIMEDB_REDUCER(insert_pk_simple_enum, ReducerContext ctx, SimpleEnum a, int32_t data)
{
    ctx.db[pk_simple_enum].insert(PkSimpleEnum{a, data});
}

// =============================================================================
// DELETE OPERATIONS - PRIMARY KEY
// =============================================================================

SPACETIMEDB_REDUCER(delete_pk_u8, ReducerContext ctx, uint8_t n)
{
    // Use optimized field accessor for direct index-based delete
    (void)ctx.db[pk_u8_n].delete_by_key(n);
}

SPACETIMEDB_REDUCER(delete_pk_u16, ReducerContext ctx, uint16_t n)
{
    // Use optimized field accessor for direct index-based delete
    (void)ctx.db[pk_u16_n].delete_by_key(n);
}

SPACETIMEDB_REDUCER(delete_pk_u32, ReducerContext ctx, uint32_t n)
{
    // Use optimized field accessor for direct index-based delete
    (void)ctx.db[pk_u32_n].delete_by_key(n);
}

SPACETIMEDB_REDUCER(delete_pk_u32_two, ReducerContext ctx, uint32_t n)
{
    // Use optimized field accessor for direct index-based delete
    (void)ctx.db[pk_u32_two_n].delete_by_key(n);
}

SPACETIMEDB_REDUCER(delete_pk_u64, ReducerContext ctx, uint64_t n)
{
    // Use optimized field accessor for direct index-based delete
    (void)ctx.db[pk_u64_n].delete_by_key(n);
}

SPACETIMEDB_REDUCER(delete_pk_u128, ReducerContext ctx, u128 n)
{
    // Use optimized field accessor for direct index-based delete
    (void)ctx.db[pk_u128_n].delete_by_key(n);
}

SPACETIMEDB_REDUCER(delete_pk_u256, ReducerContext ctx, u256 n)
{
    // Use optimized field accessor for direct index-based delete
    (void)ctx.db[pk_u256_n].delete_by_key(n);
}

SPACETIMEDB_REDUCER(delete_pk_i8, ReducerContext ctx, int8_t n)
{
    // Use optimized field accessor for direct index-based delete
    (void)ctx.db[pk_i8_n].delete_by_key(n);
}

SPACETIMEDB_REDUCER(delete_pk_i16, ReducerContext ctx, int16_t n)
{
    // Use optimized field accessor for direct index-based delete
    (void)ctx.db[pk_i16_n].delete_by_key(n);
}

SPACETIMEDB_REDUCER(delete_pk_i32, ReducerContext ctx, int32_t n)
{
    // Use optimized field accessor for direct index-based delete
    (void)ctx.db[pk_i32_n].delete_by_key(n);
}

SPACETIMEDB_REDUCER(delete_pk_i64, ReducerContext ctx, int64_t n)
{
    // Use optimized field accessor for direct index-based delete
    (void)ctx.db[pk_i64_n].delete_by_key(n);
}

SPACETIMEDB_REDUCER(delete_pk_i128, ReducerContext ctx, i128 n)
{
    // Use optimized field accessor for direct index-based delete
    (void)ctx.db[pk_i128_n].delete_by_key(n);
}

SPACETIMEDB_REDUCER(delete_pk_i256, ReducerContext ctx, i256 n)
{
    // Use optimized field accessor for direct index-based delete
    (void)ctx.db[pk_i256_n].delete_by_key(n);
}

SPACETIMEDB_REDUCER(delete_pk_bool, ReducerContext ctx, bool b)
{
    // Use optimized field accessor for direct index-based delete
    (void)ctx.db[pk_bool_b].delete_by_key(b);
}

SPACETIMEDB_REDUCER(delete_pk_string, ReducerContext ctx, std::string s)
{
    // Use optimized field accessor for direct index-based delete
    (void)ctx.db[pk_string_s].delete_by_key(s);
}

SPACETIMEDB_REDUCER(delete_pk_identity, ReducerContext ctx, Identity i)
{
    // Use optimized field accessor for direct index-based delete
    (void)ctx.db[pk_identity_i].delete_by_key(i);
}

SPACETIMEDB_REDUCER(delete_pk_connection_id, ReducerContext ctx, ConnectionId a)
{
    // Use optimized field accessor for direct index-based delete
    (void)ctx.db[pk_connection_id_a].delete_by_key(a);
}

// =============================================================================
// DELETE OPERATIONS - UNIQUE CONSTRAINT
// =============================================================================

SPACETIMEDB_REDUCER(delete_unique_u8, ReducerContext ctx, uint8_t n)
{
    // Use optimized field accessor for direct index-based delete
    ctx.db[unique_u8_n].delete_by_value(n);
}

SPACETIMEDB_REDUCER(delete_unique_u16, ReducerContext ctx, uint16_t n)
{
    // Use optimized field accessor for direct index-based delete
    ctx.db[unique_u16_n].delete_by_value(n);
}

SPACETIMEDB_REDUCER(delete_unique_u32, ReducerContext ctx, uint32_t n)
{
    // Use optimized field accessor for direct index-based delete
    ctx.db[unique_u32_n].delete_by_value(n);
}

SPACETIMEDB_REDUCER(delete_unique_u64, ReducerContext ctx, uint64_t n)
{
    // Use optimized field accessor for direct index-based delete
    ctx.db[unique_u64_n].delete_by_value(n);
}

SPACETIMEDB_REDUCER(delete_unique_u128, ReducerContext ctx, u128 n)
{
    // Use optimized field accessor for direct index-based delete
    ctx.db[unique_u128_n].delete_by_value(n);
}

SPACETIMEDB_REDUCER(delete_unique_u256, ReducerContext ctx, u256 n)
{
    // Use optimized field accessor for direct index-based delete
    ctx.db[unique_u256_n].delete_by_value(n);
}

SPACETIMEDB_REDUCER(delete_unique_i8, ReducerContext ctx, int8_t n)
{
    // Use optimized field accessor for direct index-based delete
    ctx.db[unique_i8_n].delete_by_value(n);
}

SPACETIMEDB_REDUCER(delete_unique_i16, ReducerContext ctx, int16_t n)
{
    // Use optimized field accessor for direct index-based delete
    ctx.db[unique_i16_n].delete_by_value(n);
}

SPACETIMEDB_REDUCER(delete_unique_i32, ReducerContext ctx, int32_t n)
{
    // Use optimized field accessor for direct index-based delete
    ctx.db[unique_i32_n].delete_by_value(n);
}

SPACETIMEDB_REDUCER(delete_unique_i64, ReducerContext ctx, int64_t n)
{
    // Use optimized field accessor for direct index-based delete
    ctx.db[unique_i64_n].delete_by_value(n);
}

SPACETIMEDB_REDUCER(delete_unique_i128, ReducerContext ctx, i128 n)
{
    // Use optimized field accessor for direct index-based delete
    ctx.db[unique_i128_n].delete_by_value(n);
}

SPACETIMEDB_REDUCER(delete_unique_i256, ReducerContext ctx, i256 n)
{
    // Use optimized field accessor for direct index-based delete
    ctx.db[unique_i256_n].delete_by_value(n);
}

SPACETIMEDB_REDUCER(delete_unique_bool, ReducerContext ctx, bool b)
{
    // Use optimized field accessor for direct index-based delete
    ctx.db[unique_bool_b].delete_by_value(b);
}

SPACETIMEDB_REDUCER(delete_unique_string, ReducerContext ctx, std::string s)
{
    // Use optimized field accessor for direct index-based delete
    ctx.db[unique_string_s].delete_by_value(s);
}

SPACETIMEDB_REDUCER(delete_unique_identity, ReducerContext ctx, Identity i)
{
    // Use optimized field accessor for direct index-based delete
    ctx.db[unique_identity_i].delete_by_value(i);
}

SPACETIMEDB_REDUCER(delete_unique_connection_id, ReducerContext ctx, ConnectionId a)
{
    // Use optimized field accessor for direct index-based delete
    ctx.db[unique_connection_id_a].delete_by_value(a);
}

// =============================================================================
// UPDATE OPERATIONS - PRIMARY KEY
// =============================================================================

SPACETIMEDB_REDUCER(update_pk_u8, ReducerContext ctx, uint8_t n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    (void)ctx.db[pk_u8_n].update(PkU8{n, data});
}

SPACETIMEDB_REDUCER(update_pk_u16, ReducerContext ctx, uint16_t n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    (void)ctx.db[pk_u16_n].update(PkU16{n, data});
}

SPACETIMEDB_REDUCER(update_pk_u32, ReducerContext ctx, uint32_t n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    (void)ctx.db[pk_u32_n].update(PkU32{n, data});
}

SPACETIMEDB_REDUCER(update_pk_u32_two, ReducerContext ctx, uint32_t n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    (void)ctx.db[pk_u32_two_n].update(PkU32Two{n, data});
}

SPACETIMEDB_REDUCER(update_pk_u64, ReducerContext ctx, uint64_t n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    (void)ctx.db[pk_u64_n].update(PkU64{n, data});
}

SPACETIMEDB_REDUCER(update_pk_u128, ReducerContext ctx, u128 n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    (void)ctx.db[pk_u128_n].update(PkU128{n, data});
}

SPACETIMEDB_REDUCER(update_pk_u256, ReducerContext ctx, u256 n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    (void)ctx.db[pk_u256_n].update(PkU256{n, data});
}

SPACETIMEDB_REDUCER(update_pk_i8, ReducerContext ctx, int8_t n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    (void)ctx.db[pk_i8_n].update(PkI8{n, data});
}

SPACETIMEDB_REDUCER(update_pk_i16, ReducerContext ctx, int16_t n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    (void)ctx.db[pk_i16_n].update(PkI16{n, data});
}

SPACETIMEDB_REDUCER(update_pk_i32, ReducerContext ctx, int32_t n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    (void)ctx.db[pk_i32_n].update(PkI32{n, data});
}

SPACETIMEDB_REDUCER(update_pk_i64, ReducerContext ctx, int64_t n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    (void)ctx.db[pk_i64_n].update(PkI64{n, data});
}

SPACETIMEDB_REDUCER(update_pk_i128, ReducerContext ctx, i128 n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    (void)ctx.db[pk_i128_n].update(PkI128{n, data});
}

SPACETIMEDB_REDUCER(update_pk_i256, ReducerContext ctx, i256 n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    (void)ctx.db[pk_i256_n].update(PkI256{n, data});
}

SPACETIMEDB_REDUCER(update_pk_bool, ReducerContext ctx, bool b, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    (void)ctx.db[pk_bool_b].update(PkBool{b, data});
}

SPACETIMEDB_REDUCER(update_pk_string, ReducerContext ctx, std::string s, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    (void)ctx.db[pk_string_s].update(PkString{s, data});
}

SPACETIMEDB_REDUCER(update_pk_identity, ReducerContext ctx, Identity i, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    (void)ctx.db[pk_identity_i].update(PkIdentity{i, data});
}

SPACETIMEDB_REDUCER(update_pk_connection_id, ReducerContext ctx, ConnectionId a, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    (void)ctx.db[pk_connection_id_a].update(PkConnectionId{a, data});
}

SPACETIMEDB_REDUCER(update_pk_simple_enum, ReducerContext ctx, SimpleEnum a, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    (void)ctx.db[pk_simple_enum_a].update(PkSimpleEnum{a, data});
}

// =============================================================================
// UPDATE OPERATIONS - UNIQUE CONSTRAINT
// =============================================================================

SPACETIMEDB_REDUCER(update_unique_u8, ReducerContext ctx, uint8_t n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    ctx.db[unique_u8_n].update(UniqueU8{n, data});
}

SPACETIMEDB_REDUCER(update_unique_u16, ReducerContext ctx, uint16_t n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    ctx.db[unique_u16_n].update(UniqueU16{n, data});
}

SPACETIMEDB_REDUCER(update_unique_u32, ReducerContext ctx, uint32_t n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    ctx.db[unique_u32_n].update(UniqueU32{n, data});
}

SPACETIMEDB_REDUCER(update_unique_u64, ReducerContext ctx, uint64_t n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    ctx.db[unique_u64_n].update(UniqueU64{n, data});
}

SPACETIMEDB_REDUCER(update_unique_u128, ReducerContext ctx, u128 n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    ctx.db[unique_u128_n].update(UniqueU128{n, data});
}

SPACETIMEDB_REDUCER(update_unique_u256, ReducerContext ctx, u256 n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    ctx.db[unique_u256_n].update(UniqueU256{n, data});
}

SPACETIMEDB_REDUCER(update_unique_i8, ReducerContext ctx, int8_t n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    ctx.db[unique_i8_n].update(UniqueI8{n, data});
}

SPACETIMEDB_REDUCER(update_unique_i16, ReducerContext ctx, int16_t n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    ctx.db[unique_i16_n].update(UniqueI16{n, data});
}

SPACETIMEDB_REDUCER(update_unique_i32, ReducerContext ctx, int32_t n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    ctx.db[unique_i32_n].update(UniqueI32{n, data});
}

SPACETIMEDB_REDUCER(update_unique_i64, ReducerContext ctx, int64_t n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    ctx.db[unique_i64_n].update(UniqueI64{n, data});
}

SPACETIMEDB_REDUCER(update_unique_i128, ReducerContext ctx, i128 n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    ctx.db[unique_i128_n].update(UniqueI128{n, data});
}

SPACETIMEDB_REDUCER(update_unique_i256, ReducerContext ctx, i256 n, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    ctx.db[unique_i256_n].update(UniqueI256{n, data});
}

SPACETIMEDB_REDUCER(update_unique_bool, ReducerContext ctx, bool b, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    ctx.db[unique_bool_b].update(UniqueBool{b, data});
}

SPACETIMEDB_REDUCER(update_unique_string, ReducerContext ctx, std::string s, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    ctx.db[unique_string_s].update(UniqueString{s, data});
}

SPACETIMEDB_REDUCER(update_unique_identity, ReducerContext ctx, Identity i, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    ctx.db[unique_identity_i].update(UniqueIdentity{i, data});
}

SPACETIMEDB_REDUCER(update_unique_connection_id, ReducerContext ctx, ConnectionId a, int32_t data)
{
    // Use optimized field accessor for direct index-based update
    ctx.db[unique_connection_id_a].update(UniqueConnectionId{a, data});
}

// =============================================================================
// COMPREHENSIVE TABLE REDUCERS
// =============================================================================

 SPACETIMEDB_REDUCER(insert_large_table, ReducerContext ctx,
    uint8_t a, uint16_t b, uint32_t c, uint64_t d, u128 e, u256 f,
    int8_t g, int16_t h, int32_t i, int64_t j, i128 k, i256 l,
    bool m, float n, double o, std::string p,
    SimpleEnum q, EnumWithPayload r, UnitStruct s, ByteStruct t,
    EveryPrimitiveStruct u, EveryVecStruct v)
{
    ctx.db[large_table].insert(LargeTable{
        a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r, s, t, u, v
    });
} 

 SPACETIMEDB_REDUCER(delete_large_table, ReducerContext ctx,
    uint8_t a, uint16_t b, uint32_t c, uint64_t d, u128 e, u256 f,
    int8_t g, int16_t h, int32_t i, int64_t j, i128 k, i256 l,
    bool m, float n, double o, std::string p,
    SimpleEnum q, EnumWithPayload r, UnitStruct s, ByteStruct t,
    EveryPrimitiveStruct u, EveryVecStruct v)
{
    ctx.db[large_table].delete_by_value(LargeTable{
        a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r, s, t, u, v
    });
} 

SPACETIMEDB_REDUCER(insert_table_holds_table, ReducerContext ctx, OneU8 a, VecU8 b)
{
    ctx.db[table_holds_table].insert(TableHoldsTable{a, b});
}

// =============================================================================
// SPECIAL CONTEXT REDUCERS
// =============================================================================

SPACETIMEDB_REDUCER(insert_caller_one_identity, ReducerContext ctx)
{
    ctx.db[one_identity].insert(OneIdentity{ctx.sender});
}

SPACETIMEDB_REDUCER(insert_caller_vec_identity, ReducerContext ctx)
{
    ctx.db[vec_identity].insert(VecIdentity{{ctx.sender}});
}

SPACETIMEDB_REDUCER(insert_caller_unique_identity, ReducerContext ctx, int32_t data)
{
    ctx.db[unique_identity].insert(UniqueIdentity{ctx.sender, data});
}

SPACETIMEDB_REDUCER(insert_caller_pk_identity, ReducerContext ctx, int32_t data)
{
    ctx.db[pk_identity].insert(PkIdentity{ctx.sender, data});
}

SPACETIMEDB_REDUCER(insert_caller_one_connection_id, ReducerContext ctx)
{
    if (ctx.connection_id.has_value()) {
        ctx.db[one_connection_id].insert(OneConnectionId{ctx.connection_id.value()});
    }
}

SPACETIMEDB_REDUCER(insert_caller_vec_connection_id, ReducerContext ctx)
{
    if (ctx.connection_id.has_value()) {
        ctx.db[vec_connection_id].insert(VecConnectionId{{ctx.connection_id.value()}});
    }
}

SPACETIMEDB_REDUCER(insert_caller_unique_connection_id, ReducerContext ctx, int32_t data)
{
    if (ctx.connection_id.has_value()) {
        ctx.db[unique_connection_id].insert(UniqueConnectionId{ctx.connection_id.value(), data});
    }
}

SPACETIMEDB_REDUCER(insert_caller_pk_connection_id, ReducerContext ctx, int32_t data)
{
    if (ctx.connection_id.has_value()) {
        ctx.db[pk_connection_id].insert(PkConnectionId{ctx.connection_id.value(), data});
    }
}

SPACETIMEDB_REDUCER(insert_call_timestamp, ReducerContext ctx)
{
    ctx.db[one_timestamp].insert(OneTimestamp{ctx.timestamp});
}

SPACETIMEDB_REDUCER(insert_primitives_as_strings, ReducerContext ctx, EveryPrimitiveStruct s)
{
    // Helper function to format floats like Rust (no trailing zeros)
    auto format_float = [](float f) -> std::string {
        if (f == 1.0f) return "1";
        if (f == -1.0f) return "-1";
        return std::to_string(f);
    };
    
    std::vector<std::string> string_values = {
        std::to_string(s.a), std::to_string(s.b), std::to_string(s.c), std::to_string(s.d),
        s.e.to_string(), s.f.to_string(),
        std::to_string(s.g), std::to_string(s.h), std::to_string(s.i), std::to_string(s.j),
        s.k.to_string(), s.l.to_string(),
        s.m ? "true" : "false", format_float(s.n), format_float(s.o), s.p,
        s.q.to_string(), s.r.to_string(), s.s.to_string(), s.t.to_string()
    };
    ctx.db[vec_string].insert(VecString{string_values});
}

// =============================================================================
// SPECIALIZED OPERATIONS
// =============================================================================

SPACETIMEDB_REDUCER(insert_into_btree_u32, ReducerContext ctx, std::vector<BTreeU32> rows)
{
    for (const auto& row : rows) {
        ctx.db[btree_u32].insert(row);
    }
}

SPACETIMEDB_REDUCER(delete_from_btree_u32, ReducerContext ctx, std::vector<BTreeU32> rows)
{
    for (const auto& row : rows) {
        ctx.db[btree_u32].delete_by_value(row);
    }
}

SPACETIMEDB_REDUCER(insert_into_pk_btree_u32, ReducerContext ctx, std::vector<PkU32> pk_u32, std::vector<BTreeU32> bt_u32)
{
    for (const auto& row : pk_u32) {
        ctx.db[::pk_u32].insert(row);
    }
    for (const auto& row : bt_u32) {
        ctx.db[btree_u32].insert(row);
    }
}

SPACETIMEDB_REDUCER(insert_unique_u32_update_pk_u32, ReducerContext ctx, uint32_t n, int32_t d_unique, int32_t d_pk)
{
    ctx.db[unique_u32].insert(UniqueU32{n, d_unique});
    // Use the update method via field accessor for primary key tables
    (void)ctx.db[pk_u32_n].update(PkU32{n, d_pk});
}

SPACETIMEDB_REDUCER(delete_pk_u32_insert_pk_u32_two, ReducerContext ctx, uint32_t n, int32_t data)
{
    ctx.db[pk_u32_two].insert(PkU32Two{n, data});
    (void)ctx.db[pk_u32_n].delete_by_key(n);
}

SPACETIMEDB_REDUCER(insert_user, ReducerContext ctx, std::string name, Identity identity)
{
    ctx.db[users].insert(Users{identity, name});
}

SPACETIMEDB_REDUCER(insert_into_indexed_simple_enum, ReducerContext ctx, SimpleEnum n)
{
    ctx.db[indexed_simple_enum].insert(IndexedSimpleEnum{n});
}

SPACETIMEDB_REDUCER(update_indexed_simple_enum, ReducerContext ctx, SimpleEnum a, SimpleEnum b)
{
    auto table = ctx.db[indexed_simple_enum];
    
    // Find and delete rows with value a, then insert row with value b
    for (auto& row : table) {
        if (row.n == a) {
            table.delete_by_value(row);
            table.insert(IndexedSimpleEnum{b});
            break;  // Only update the first match
        }
    }
}


// Scheduled table operations
SPACETIMEDB_REDUCER(send_scheduled_message, ReducerContext ctx, ScheduledTable arg)
{
    LOG_INFO_F("Scheduled message executed: ID=%llu, text=%s", arg.scheduled_id, arg.text.c_str());
}

// =============================================================================
// CLIENT VISIBILITY FILTERS
// =============================================================================

SPACETIMEDB_CLIENT_VISIBILITY_FILTER(
    one_u8_visible,
    "SELECT * FROM one_u8"
)

SPACETIMEDB_CLIENT_VISIBILITY_FILTER(
    users_filter,
    "SELECT * FROM users WHERE identity = :sender"
)

// =============================================================================
// NO-OP REDUCER FOR TESTING
// =============================================================================

SPACETIMEDB_REDUCER(no_op_succeeds, ReducerContext ctx)
{
    LOG_INFO("No-op reducer executed successfully");
}
