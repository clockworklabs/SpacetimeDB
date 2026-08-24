use spacetimedb::{reducer, table, Identity, ReducerContext, Table};

#[table(
    accessor = blob_record,
    public,
    index(accessor = by_owner, btree(columns = [owner]))
)]
pub struct BlobRecord {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub owner: Identity,
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
    pub data: Vec<u8>,
}

#[reducer]
pub fn store_blob(ctx: &ReducerContext, filename: String, mime_type: String, data: Vec<u8>) {
    ctx.db.blob_record().insert(BlobRecord {
        id: 0,
        owner: ctx.sender(),
        filename,
        mime_type,
        size: data.len() as u64,
        data,
    });
}
