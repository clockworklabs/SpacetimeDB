use serde::{Serialize, Deserialize};
use spacetimedb_bindings::TypeValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageJson {
    FunctionCall(FunctionCallJson),
    SubscriptionUpdate(SubscriptionUpdateJson),
    Event(EventJson),
    TransactionUpdate(TransactionUpdateJson),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCallJson {
    pub reducer: String,
    pub arg_bytes: Vec<u8>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableUpdateJson {
    pub table_id: u32,
    pub table_row_operations: Vec<TableRowOperationJson>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableRowOperationJson {
    pub op: String,
    pub row_pk: String,
    pub row: Vec<TypeValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionUpdateJson {
    pub table_updates: Vec<TableUpdateJson>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventJson {
    pub timestamp: u64,
    pub status: String, // committed, failed
    pub caller_identity: String, // hex identity
    pub function_call: FunctionCallJson
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionUpdateJson {
    pub event: EventJson,
    pub subscription_update: SubscriptionUpdateJson,
}