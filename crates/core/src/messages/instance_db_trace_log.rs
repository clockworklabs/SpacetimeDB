use spacetimedb_primitives::TableId;
use spacetimedb_sats::de::Deserialize;
use spacetimedb_sats::ser::Serialize;

use crate::host::Timestamp;

#[derive(Clone, Serialize, Deserialize)]
pub struct Insert {
    pub table_id: TableId,
    pub buffer: Vec<u8>,
}
/*
#[derive(Clone, Serialize, Deserialize)]
pub struct DeletePk {
    pub table_id: TableId,
    pub buffer: Vec<u8>,
    pub result_success: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct DeleteValue {
    pub table_id: TableId,
    pub buffer: Vec<u8>,
    pub result_success: bool,
}
*/
#[derive(Clone, Serialize, Deserialize)]
pub struct DeleteByColEq {
    pub table_id: TableId,
    pub col_id: u32,
    pub buffer: Vec<u8>,
    pub result_deleted_count: u32,
}
/*
#[derive(Clone, Serialize, Deserialize)]
pub struct DeleteRange {
    pub table_id: TableId,
    pub cols: u32,
    pub start_buffer: Vec<u8>,
    pub end_buffer: Vec<u8>,
    pub result_deleted_count: u32,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct CreateTable {
    pub table_name: String,
    pub schema_buffer: Vec<u8>,
    pub result_table_id: u32,
}
*/
#[derive(Clone, Serialize, Deserialize)]
pub struct GetTableId {
    pub table_name: String,
    pub result_table_id: u32,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Iter {
    pub table_id: TableId,
    pub result_bytes: Vec<u8>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct CreateIndex {
    pub index_name: String,
    pub table_id: TableId,
    pub index_type: u32,
    pub col_ids: Vec<u32>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct InstanceEvent {
    pub event_start_epoch_micros: Timestamp,
    pub duration_micros: u64,
    pub r#type: InstanceEventType,
}
#[derive(Clone, Serialize, Deserialize)]
pub enum InstanceEventType {
    Insert(Insert),
    DeleteByColEq(DeleteByColEq),
    /*
    DeletePk(DeletePk),
    DeleteValue(DeleteValue),
    DeleteRange(DeleteRange),
    CreateTable(CreateTable),
    */
    GetTableId(GetTableId),
    Iter(Iter),
    CreateIndex(CreateIndex),
}
