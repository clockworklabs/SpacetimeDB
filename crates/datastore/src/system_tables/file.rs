//! File Storage: the records of files whose bytes live in the database's file store.
//!
//! `st_file` holds one row per file and is the only strong reference to a file's bytes.
//! Module code reaches it only through the host's file syscalls, because its rows hold read secrets
//! and because the host must not lose track of which content hashes are live.
//!
//! Deleting a file marks its row with the offset of the deleting transaction rather than removing it.
//! A marked row is no longer a file to modules, but it keeps the bytes alive, and every snapshot lists
//! them, for as long as history from before the deletion can be reverted to. Once that history is gone,
//! the host purges the row, and reclamation can then delete the bytes.
//!
//! `st_file_staged` holds the bytes of files that a reducer created and the host has not yet persisted
//! to the file store. Keeping them in a table puts them in the commitlog with the transaction that
//! created them, so a crash before the upload finishes loses nothing. The host deletes the row once
//! the bytes are persisted.
use super::*;
use spacetimedb_lib::FileId;

pub const ST_FILE_ID: TableId = TableId(26);
pub const ST_FILE_NAME: &str = "st_file";
pub const ST_FILE_STAGED_ID: TableId = TableId(27);
pub const ST_FILE_STAGED_NAME: &str = "st_file_staged";
pub const ST_UPLOAD_ID: TableId = TableId(28);
pub const ST_UPLOAD_NAME: &str = "st_upload";

st_fields_enum!(enum StFileFields {
    "file_id", FileId = 0,
    "read_secret", ReadSecret = 1,
    "content_hash", ContentHash = 2,
    "size", Size = 3,
    "created_at", CreatedAt = 4,
    "content_type", ContentType = 5,
    "deleted_at_offset", DeletedAtOffset = 6,
});

/// A row of `st_file`: one stored file.
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StFileRow {
    pub file_id: FileId,
    /// The secret a `FileRef` must carry to read this file.
    pub read_secret: u128,
    /// The BLAKE3 hash of the file's contents, and its key in the file store.
    pub content_hash: Hash,
    pub size: u64,
    pub created_at: TimestampViaI64,
    /// Declared by the uploader or module, and never verified.
    pub content_type: Option<String>,
    /// The offset of the transaction that deleted this file, or `None` while it is live.
    pub deleted_at_offset: Option<u64>,
}

impl StFileRow {
    /// Whether the file hasn't been deleted.
    pub fn is_live(&self) -> bool {
        self.deleted_at_offset.is_none()
    }

    pub fn file_ref(&self) -> spacetimedb_lib::FileRef {
        spacetimedb_lib::FileRef::new(self.file_id, self.read_secret)
    }

    pub fn metadata(&self) -> spacetimedb_lib::FileMetadata {
        spacetimedb_lib::FileMetadata {
            file_ref: self.file_ref(),
            content_hash: self.content_hash,
            size: self.size,
            created_at: self.created_at.0,
            content_type: self.content_type.clone(),
        }
    }
}

impl TryFrom<RowRef<'_>> for StFileRow {
    type Error = DatastoreError;
    fn try_from(row: RowRef<'_>) -> Result<Self, Self::Error> {
        read_via_bsatn(row)
    }
}

impl From<StFileRow> for ProductValue {
    fn from(row: StFileRow) -> Self {
        to_product_value(&row)
    }
}

st_fields_enum!(enum StFileStagedFields {
    "content_hash", ContentHash = 0,
    "bytes", Bytes = 1,
});

/// A row of `st_file_staged`: the bytes of a file created in a reducer, until they are persisted.
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StFileStagedRow {
    pub content_hash: Hash,
    pub bytes: Box<[u8]>,
}

impl TryFrom<RowRef<'_>> for StFileStagedRow {
    type Error = DatastoreError;
    fn try_from(row: RowRef<'_>) -> Result<Self, Self::Error> {
        read_via_bsatn(row)
    }
}

impl From<StFileStagedRow> for ProductValue {
    fn from(row: StFileStagedRow) -> Self {
        to_product_value(&row)
    }
}

st_fields_enum!(enum StUploadFields {
    "sender", Sender = 0,
    "idempotency_key", IdempotencyKey = 1,
    "file_id", FileId = 2,
    "completed_at", CompletedAt = 3,
});

/// A row of `st_upload`: an upload that completed with an `Idempotency-Key`, so that retrying it
/// returns the same file instead of creating another. Rows expire after a day.
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StUploadRow {
    /// The uploader. Keys are scoped to their sender.
    pub sender: IdentityViaU256,
    pub idempotency_key: String,
    /// The file the upload created.
    pub file_id: FileId,
    pub completed_at: TimestampViaI64,
}

impl TryFrom<RowRef<'_>> for StUploadRow {
    type Error = DatastoreError;
    fn try_from(row: RowRef<'_>) -> Result<Self, Self::Error> {
        read_via_bsatn(row)
    }
}

impl From<StUploadRow> for ProductValue {
    fn from(row: StUploadRow) -> Self {
        to_product_value(&row)
    }
}

pub(super) fn register_tables(builder: &mut RawModuleDefV9Builder) {
    let ty = builder.add_type::<StFileRow>();
    builder
        .build_table(ST_FILE_NAME, *ty.as_ref().expect("system row must be a product"))
        .with_type(TableType::System)
        .with_access(v9::TableAccess::Private)
        .with_primary_key(StFileFields::FileId)
        .with_unique_constraint(StFileFields::FileId)
        .with_index_no_accessor_name(btree(StFileFields::FileId))
        .with_index_no_accessor_name(btree(StFileFields::ContentHash));

    let ty = builder.add_type::<StFileStagedRow>();
    builder
        .build_table(ST_FILE_STAGED_NAME, *ty.as_ref().expect("system row must be a product"))
        .with_type(TableType::System)
        .with_access(v9::TableAccess::Private)
        .with_primary_key(StFileStagedFields::ContentHash)
        .with_unique_constraint(StFileStagedFields::ContentHash)
        .with_index_no_accessor_name(btree(StFileStagedFields::ContentHash));

    let ty = builder.add_type::<StUploadRow>();
    let key = col_list![StUploadFields::Sender, StUploadFields::IdempotencyKey];
    builder
        .build_table(ST_UPLOAD_NAME, *ty.as_ref().expect("system row must be a product"))
        .with_type(TableType::System)
        .with_access(v9::TableAccess::Private)
        .with_unique_constraint(key.clone())
        .with_index_no_accessor_name(btree(key));
}

pub(crate) fn st_file_schema() -> TableSchema {
    st_schema(ST_FILE_NAME, ST_FILE_ID)
}

pub(crate) fn st_file_staged_schema() -> TableSchema {
    st_schema(ST_FILE_STAGED_NAME, ST_FILE_STAGED_ID)
}

pub(crate) fn st_upload_schema() -> TableSchema {
    st_schema(ST_UPLOAD_NAME, ST_UPLOAD_ID)
}

/// The file tables are reachable from module code only through the file syscalls.
pub(super) fn is_file_table(table: TableId) -> bool {
    matches!(table, ST_FILE_ID | ST_FILE_STAGED_ID | ST_UPLOAD_ID)
}

/// The indexes of [`is_file_table`] tables, as assigned in `INDEX_IDS`.
pub(super) fn is_file_index(index: IndexId) -> bool {
    matches!(index, IndexId(36..=39))
}
