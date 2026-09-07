//! Stable system schemas for environment and container deployment state.
//!
//! The deployment payload and operation result have versioned binary encodings,
//! so adding a protocol version does not change the system table row layout.
//! Publication and target fences are operational metadata: application restore
//! must preserve/reconcile their current authority before admitting execution.

use super::*;

pub const ST_ENV_ID: TableId = TableId(21);
pub const ST_DEPLOYMENT_ID: TableId = TableId(22);
pub const ST_PUBLISH_FENCE_ID: TableId = TableId(23);
pub const ST_DEPLOYMENT_OPERATION_ID: TableId = TableId(24);
pub const ST_CONTAINER_FENCE_ID: TableId = TableId(25);
pub const ST_CONNECTION_AUTH_ID: TableId = TableId(26);

pub const ST_ENV_NAME: &str = "st_env";
pub const ST_DEPLOYMENT_NAME: &str = "st_deployment";
pub const ST_PUBLISH_FENCE_NAME: &str = "st_publish_fence";
pub const ST_DEPLOYMENT_OPERATION_NAME: &str = "st_deployment_operation";
pub const ST_CONTAINER_FENCE_NAME: &str = "st_container_fence";
pub const ST_CONNECTION_AUTH_NAME: &str = "st_connection_auth";

st_fields_enum!(enum StEnvFields {
    "key", Key = 0,
    "value", Value = 1,
});
st_fields_enum!(enum StDeploymentFields {
    "key", Key = 0,
    "revision", Revision = 1,
    "last_operation_id", LastOperationId = 2,
    "payload", Payload = 3,
});
st_fields_enum!(enum StPublishFenceFields {
    "key", Key = 0,
    "publication_epoch", PublicationEpoch = 1,
    "operation_id", OperationId = 2,
});
st_fields_enum!(enum StDeploymentOperationFields {
    "operation_id", OperationId = 0,
    "previous_revision", PreviousRevision = 1,
    "committed_revision", CommittedRevision = 2,
    "commit_result", CommitResult = 3,
    "expires_at", ExpiresAt = 4,
});
st_fields_enum!(enum StContainerFenceFields {
    "source_identity", SourceIdentity = 0,
    "generation", Generation = 1,
    "target_grant_revision", TargetGrantRevision = 2,
    "target_set_hash", TargetSetHash = 3,
    "allowed", Allowed = 4,
});
st_fields_enum!(enum StConnectionAuthFields {
    "connection_id", ConnectionId = 0,
    "sender_identity", SenderIdentity = 1,
    "call_auth_flags", CallAuthFlags = 2,
});

#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StEnvRow {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StDeploymentRow {
    pub key: u8,
    pub revision: Hash,
    pub last_operation_id: u128,
    pub payload: Box<[u8]>,
}

#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StPublishFenceRow {
    pub key: u8,
    pub publication_epoch: u64,
    pub operation_id: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StDeploymentOperationRow {
    pub operation_id: u128,
    pub previous_revision: Option<Hash>,
    pub committed_revision: Hash,
    pub commit_result: Box<[u8]>,
    pub expires_at: TimestampViaI64,
}

#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StContainerFenceRow {
    pub source_identity: IdentityViaU256,
    pub generation: u64,
    pub target_grant_revision: u64,
    pub target_set_hash: Hash,
    pub allowed: bool,
}

/// Captured host authentication for lifecycle cleanup, including crash recovery.
/// Only hosted connections need a row; absent rows retain ordinary flags zero.
/// JWT claims and sender equality never reconstruct these flags.
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StConnectionAuthRow {
    pub connection_id: ConnectionIdViaU128,
    pub sender_identity: IdentityViaU256,
    pub call_auth_flags: u32,
}

macro_rules! row_conversions {
    ($($row:ty),+ $(,)?) => {$ (
        impl TryFrom<RowRef<'_>> for $row {
            type Error = DatastoreError;
            fn try_from(row: RowRef<'_>) -> Result<Self, Self::Error> {
                read_via_bsatn(row)
            }
        }
        impl From<$row> for ProductValue {
            fn from(row: $row) -> Self { to_product_value(&row) }
        }
    )+};
}

row_conversions!(
    StEnvRow,
    StDeploymentRow,
    StPublishFenceRow,
    StDeploymentOperationRow,
    StContainerFenceRow,
    StConnectionAuthRow
);

pub(super) fn register_tables(builder: &mut RawModuleDefV9Builder) {
    fn register<T: SpacetimeType>(builder: &mut RawModuleDefV9Builder, name: &'static str) {
        let ty = builder.add_type::<T>();
        builder
            .build_table(name, *ty.as_ref().expect("system row must be a product"))
            .with_type(TableType::System)
            .with_access(v9::TableAccess::Private)
            .with_primary_key(ColId(0))
            .with_unique_constraint(ColId(0))
            .with_index_no_accessor_name(btree(ColId(0)));
    }
    register::<StEnvRow>(builder, ST_ENV_NAME);
    register::<StDeploymentRow>(builder, ST_DEPLOYMENT_NAME);
    register::<StPublishFenceRow>(builder, ST_PUBLISH_FENCE_NAME);
    register::<StDeploymentOperationRow>(builder, ST_DEPLOYMENT_OPERATION_NAME);
    register::<StContainerFenceRow>(builder, ST_CONTAINER_FENCE_NAME);
    register::<StConnectionAuthRow>(builder, ST_CONNECTION_AUTH_NAME);
}

pub(super) fn validate_tables(def: &ModuleDef) {
    validate_system_table::<StEnvFields>(def, ST_ENV_NAME);
    validate_system_table::<StDeploymentFields>(def, ST_DEPLOYMENT_NAME);
    validate_system_table::<StPublishFenceFields>(def, ST_PUBLISH_FENCE_NAME);
    validate_system_table::<StDeploymentOperationFields>(def, ST_DEPLOYMENT_OPERATION_NAME);
    validate_system_table::<StContainerFenceFields>(def, ST_CONTAINER_FENCE_NAME);
    validate_system_table::<StConnectionAuthFields>(def, ST_CONNECTION_AUTH_NAME);
}

pub(crate) fn deployment_system_schemas() -> [TableSchema; 6] {
    [
        st_schema(ST_ENV_NAME, ST_ENV_ID),
        st_schema(ST_DEPLOYMENT_NAME, ST_DEPLOYMENT_ID),
        st_schema(ST_PUBLISH_FENCE_NAME, ST_PUBLISH_FENCE_ID),
        st_schema(ST_DEPLOYMENT_OPERATION_NAME, ST_DEPLOYMENT_OPERATION_ID),
        st_schema(ST_CONTAINER_FENCE_NAME, ST_CONTAINER_FENCE_ID),
        st_schema(ST_CONNECTION_AUTH_NAME, ST_CONNECTION_AUTH_ID),
    ]
}

pub(super) fn system_schema(table: TableId) -> Option<TableSchema> {
    let name = match table {
        ST_ENV_ID => ST_ENV_NAME,
        ST_DEPLOYMENT_ID => ST_DEPLOYMENT_NAME,
        ST_PUBLISH_FENCE_ID => ST_PUBLISH_FENCE_NAME,
        ST_DEPLOYMENT_OPERATION_ID => ST_DEPLOYMENT_OPERATION_NAME,
        ST_CONTAINER_FENCE_ID => ST_CONTAINER_FENCE_NAME,
        ST_CONNECTION_AUTH_ID => ST_CONNECTION_AUTH_NAME,
        _ => return None,
    };
    Some(st_schema(name, table))
}

/// These tables are read through dedicated host operations by module code.
/// In particular, resolving a numeric table or index ID must not bypass this.
pub fn is_module_restricted_table(table: TableId) -> bool {
    matches!(
        table,
        ST_ENV_ID
            | ST_DEPLOYMENT_ID
            | ST_PUBLISH_FENCE_ID
            | ST_DEPLOYMENT_OPERATION_ID
            | ST_CONTAINER_FENCE_ID
            | ST_CONNECTION_AUTH_ID
    )
}

pub fn is_module_restricted_index(index: IndexId) -> bool {
    INDEXES.iter().any(|(_, restricted)| *restricted == index)
}

/// Environment management has its own validated SQL path. Deployment and
/// authorization metadata may only be changed by authenticated host operations.
pub fn is_host_managed_deployment_table(table: TableId) -> bool {
    matches!(
        table,
        ST_DEPLOYMENT_ID
            | ST_PUBLISH_FENCE_ID
            | ST_DEPLOYMENT_OPERATION_ID
            | ST_CONTAINER_FENCE_ID
            | ST_CONNECTION_AUTH_ID
    )
}

pub(super) const CONSTRAINTS: [(&str, ConstraintId); 6] = [
    ("st_env_key_key", ConstraintId(26)),
    ("st_deployment_key_key", ConstraintId(27)),
    ("st_publish_fence_key_key", ConstraintId(28)),
    ("st_deployment_operation_operation_id_key", ConstraintId(29)),
    ("st_container_fence_source_identity_key", ConstraintId(30)),
    ("st_connection_auth_connection_id_key", ConstraintId(31)),
];

pub(super) const INDEXES: [(&str, IndexId); 6] = [
    ("st_env_key_idx_btree", IndexId(30)),
    ("st_deployment_key_idx_btree", IndexId(31)),
    ("st_publish_fence_key_idx_btree", IndexId(32)),
    ("st_deployment_operation_operation_id_idx_btree", IndexId(33)),
    ("st_container_fence_source_identity_idx_btree", IndexId(34)),
    ("st_connection_auth_connection_id_idx_btree", IndexId(35)),
];
