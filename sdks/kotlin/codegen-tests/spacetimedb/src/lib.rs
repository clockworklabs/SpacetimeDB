use spacetimedb::{ReducerContext, SpacetimeType, Table};

// --- Edge-case types for Kotlin codegen verification ---

/// Empty product type — should generate `data object` in Kotlin.
#[derive(SpacetimeType)]
pub struct UnitStruct {}

/// Table referencing the empty product type so it gets exported.
#[spacetimedb::table(accessor = unit_test_row, public)]
pub struct UnitTestRow {
    #[primary_key]
    #[auto_inc]
    id: u64,
    value: UnitStruct,
}

#[spacetimedb::reducer(init)]
pub fn init(_ctx: &ReducerContext) {}
