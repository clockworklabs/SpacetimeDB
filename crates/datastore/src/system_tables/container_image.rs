//! Container images stored in the database, as `st_module` stores the module's program.
use super::*;
// 26 is reserved for `st_outbox`.
pub const ST_CONTAINER_IMAGE_ID: TableId = TableId(27);
pub const ST_CONTAINER_IMAGE_NAME: &str = "st_container_image";
st_fields_enum!(enum StContainerImageFields {
    "image_id", ImageId = 0,
    "image", Image = 1,
    "size", Size = 2,
    "created_at", CreatedAt = 3,
});
/// System Table [ST_CONTAINER_IMAGE_NAME]
/// An image for the database's container, which the server loads into Docker when its Docker
/// daemon lacks the image.
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StContainerImageRow {
    /// The local image ID, `sha256:<64 hex digits>`.
    pub image_id: String,
    /// The gzipped output of `docker save`.
    pub image: Box<[u8]>,
    /// The length of `image`, to list images without reading them.
    pub size: u64,
    pub created_at: TimestampViaI64,
}
impl TryFrom<RowRef<'_>> for StContainerImageRow {
    type Error = DatastoreError;
    fn try_from(row: RowRef<'_>) -> Result<Self, Self::Error> {
        read_via_bsatn(row)
    }
}
impl From<StContainerImageRow> for ProductValue {
    fn from(row: StContainerImageRow) -> Self {
        to_product_value(&row)
    }
}
pub(super) fn register_table(builder: &mut RawModuleDefV9Builder) {
    let ty = builder.add_type::<StContainerImageRow>();
    builder
        .build_table(
            ST_CONTAINER_IMAGE_NAME,
            *ty.as_ref().expect("system row must be a product"),
        )
        .with_type(TableType::System)
        .with_access(v9::TableAccess::Private)
        .with_primary_key(StContainerImageFields::ImageId)
        .with_unique_constraint(StContainerImageFields::ImageId)
        .with_index_no_accessor_name(btree(StContainerImageFields::ImageId));
}
pub(crate) fn st_container_image_schema() -> TableSchema {
    st_schema(ST_CONTAINER_IMAGE_NAME, ST_CONTAINER_IMAGE_ID)
}
