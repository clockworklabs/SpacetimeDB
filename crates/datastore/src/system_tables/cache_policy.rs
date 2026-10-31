//! Per-table cache policies. Tables without a row use `RequiredInMemory`.
use super::*;
use spacetimedb_schema::schema::TableCachePolicy;

pub const ST_TABLE_CACHE_POLICY_ID: TableId = TableId(27);
pub const ST_TABLE_CACHE_POLICY_NAME: &str = "st_table_cache_policy";

st_fields_enum!(enum StTableCachePolicyFields {
    "table_id", TableId = 0,
    "policy", Policy = 1,
});

#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StTableCachePolicyRow {
    pub table_id: TableId,
    pub policy: TableCachePolicy,
}

impl TryFrom<RowRef<'_>> for StTableCachePolicyRow {
    type Error = DatastoreError;
    fn try_from(row: RowRef<'_>) -> Result<Self, Self::Error> {
        read_via_bsatn(row)
    }
}

impl From<StTableCachePolicyRow> for ProductValue {
    fn from(row: StTableCachePolicyRow) -> Self {
        to_product_value(&row)
    }
}

pub(super) fn register_table(builder: &mut RawModuleDefV9Builder) {
    let ty = builder.add_type::<StTableCachePolicyRow>();
    builder
        .build_table(
            ST_TABLE_CACHE_POLICY_NAME,
            *ty.as_ref().expect("system row must be a product"),
        )
        .with_type(TableType::System)
        .with_access(v9::TableAccess::Private)
        .with_primary_key(ColId(0))
        .with_unique_constraint(ColId(0))
        .with_index_no_accessor_name(btree(ColId(0)));
}

pub(crate) fn st_table_cache_policy_schema() -> TableSchema {
    st_schema(ST_TABLE_CACHE_POLICY_NAME, ST_TABLE_CACHE_POLICY_ID)
}
