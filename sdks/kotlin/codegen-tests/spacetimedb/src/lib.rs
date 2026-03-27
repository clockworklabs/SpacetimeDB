use spacetimedb::{
    ConnectionId, Identity, Query, ReducerContext, ScheduleAt, SpacetimeType, Table, Timestamp,
    ViewContext,
};
use spacetimedb::sats::{i256, u256};

// ─────────────────────────────────────────────────────────────────────────────
// PRODUCT TYPES
// ─────────────────────────────────────────────────────────────────────────────

/// Empty product type — should generate `data object` in Kotlin.
#[derive(SpacetimeType)]
pub struct UnitStruct {}

/// Product type with all primitive fields.
#[derive(SpacetimeType)]
pub struct AllPrimitives {
    pub val_bool: bool,
    pub val_i8: i8,
    pub val_u8: u8,
    pub val_i16: i16,
    pub val_u16: u16,
    pub val_i32: i32,
    pub val_u32: u32,
    pub val_i64: i64,
    pub val_u64: u64,
    pub val_i128: i128,
    pub val_u128: u128,
    pub val_i256: i256,
    pub val_u256: u256,
    pub val_f32: f32,
    pub val_f64: f64,
    pub val_string: String,
    pub val_bytes: Vec<u8>,
}

/// Product type with SDK-specific types.
#[derive(SpacetimeType)]
pub struct SdkTypes {
    pub identity: Identity,
    pub connection_id: ConnectionId,
    pub timestamp: Timestamp,
    pub schedule_at: ScheduleAt,
}

/// Product type with optional and nested fields.
#[derive(SpacetimeType)]
pub struct NestedTypes {
    pub optional_string: Option<String>,
    pub optional_i32: Option<i32>,
    pub list_of_strings: Vec<String>,
    pub list_of_i32: Vec<i32>,
    pub nested_struct: AllPrimitives,
    pub optional_struct: Option<SdkTypes>,
}

// ─────────────────────────────────────────────────────────────────────────────
// SUM TYPES (ENUMS)
// ─────────────────────────────────────────────────────────────────────────────

/// Plain enum — all unit variants, should generate `enum class` in Kotlin.
#[derive(SpacetimeType)]
pub enum SimpleEnum {
    Alpha,
    Beta,
    Gamma,
}

/// Mixed sum type — should generate `sealed interface` in Kotlin.
#[derive(SpacetimeType)]
pub enum MixedEnum {
    UnitVariant,
    StringVariant(String),
    IntVariant(i32),
    StructVariant(AllPrimitives),
}

// ─────────────────────────────────────────────────────────────────────────────
// TABLES
// ─────────────────────────────────────────────────────────────────────────────

/// Table referencing the empty product type.
#[spacetimedb::table(accessor = unit_test_row, public)]
pub struct UnitTestRow {
    #[primary_key]
    #[auto_inc]
    id: u64,
    value: UnitStruct,
}

/// Table with all primitive types — verifies full type mapping.
#[spacetimedb::table(accessor = all_types_row, public)]
pub struct AllTypesRow {
    #[primary_key]
    #[auto_inc]
    id: u64,
    primitives: AllPrimitives,
    sdk_types: SdkTypes,
}

/// Table with optional/nested fields.
#[spacetimedb::table(accessor = nested_row, public)]
pub struct NestedRow {
    #[primary_key]
    #[auto_inc]
    id: u64,
    data: NestedTypes,
    tag: SimpleEnum,
    payload: Option<MixedEnum>,
}

/// Table with indexes — verifies UniqueIndex and BTreeIndex codegen.
#[spacetimedb::table(
    accessor = indexed_row,
    public,
    index(accessor = name_idx, btree(columns = [name]))
)]
pub struct IndexedRow {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[unique]
    code: String,
    name: String,
}

/// Table without primary key — verifies content-key table cache.
#[spacetimedb::table(accessor = no_pk_row, public)]
pub struct NoPkRow {
    label: String,
    value: i32,
}

// ─────────────────────────────────────────────────────────────────────────────
// VIEWS
// ─────────────────────────────────────────────────────────────────────────────

/// Query-builder view over a PK table — should inherit primary key and generate
/// `RemotePersistentTableWithPrimaryKey` with `onUpdate` callbacks.
#[spacetimedb::view(accessor = all_indexed_rows, public)]
fn all_indexed_rows(ctx: &ViewContext) -> impl Query<IndexedRow> {
    ctx.from.indexed_row()
}

// ─────────────────────────────────────────────────────────────────────────────
// REDUCERS
// ─────────────────────────────────────────────────────────────────────────────

#[spacetimedb::reducer(init)]
pub fn init(_ctx: &ReducerContext) {}

/// No-arg reducer.
#[spacetimedb::reducer]
pub fn do_nothing(_ctx: &ReducerContext) {}

/// Reducer with multiple typed args.
#[spacetimedb::reducer]
pub fn insert_all_types(
    ctx: &ReducerContext,
    primitives: AllPrimitives,
    sdk_types: SdkTypes,
) {
    ctx.db.all_types_row().insert(AllTypesRow {
        id: 0,
        primitives,
        sdk_types,
    });
}

/// Reducer with enum args.
#[spacetimedb::reducer]
pub fn insert_nested(
    ctx: &ReducerContext,
    data: NestedTypes,
    tag: SimpleEnum,
    payload: Option<MixedEnum>,
) {
    ctx.db.nested_row().insert(NestedRow {
        id: 0,
        data,
        tag,
        payload,
    });
}
