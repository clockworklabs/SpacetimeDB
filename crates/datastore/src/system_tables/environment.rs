//! Private database environment state. Values follow ordinary table durability.
use super::*;
pub const ST_ENV_ID: TableId = TableId(21);
pub const ST_ENV_NAME: &str = "st_env";
st_fields_enum!(enum StEnvFields { "key", Key = 0, "value", Value = 1, });
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StEnvRow {
    pub key: String,
    pub value: String,
}
impl TryFrom<RowRef<'_>> for StEnvRow {
    type Error = DatastoreError;
    fn try_from(row: RowRef<'_>) -> Result<Self, Self::Error> {
        read_via_bsatn(row)
    }
}
impl From<StEnvRow> for ProductValue {
    fn from(row: StEnvRow) -> Self {
        to_product_value(&row)
    }
}
pub(super) fn register_table(builder: &mut RawModuleDefV9Builder) {
    let ty = builder.add_type::<StEnvRow>();
    builder
        .build_table(ST_ENV_NAME, *ty.as_ref().expect("system row must be a product"))
        .with_type(TableType::System)
        .with_access(v9::TableAccess::Private)
        .with_primary_key(ColId(0))
        .with_unique_constraint(ColId(0))
        .with_index_no_accessor_name(btree(ColId(0)));
}
pub(super) fn validate_table(def: &ModuleDef) {
    validate_system_table::<StEnvFields>(def, ST_ENV_NAME);
}
pub(crate) fn st_env_schema() -> TableSchema {
    st_schema(ST_ENV_NAME, ST_ENV_ID)
}
/// Module code must use env_get even when it guesses numeric identifiers.
pub fn is_module_restricted_table(table: TableId) -> bool {
    table == ST_ENV_ID
}
pub fn is_module_restricted_index(index: IndexId) -> bool {
    index == IndexId(30)
}
