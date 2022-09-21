use serde::{Deserialize, Serialize};
use spacetimedb_lib::TupleDef;
use spacetimedb_lib::TypeValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageJson {
    FunctionCall(FunctionCallJson),
    SubscriptionUpdate(SubscriptionUpdateJson),
    Event(EventJson),
    TransactionUpdate(TransactionUpdateJson),
    IdentityToken(IdentityTokenJson),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityTokenJson {
    pub identity: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCallJson {
    pub reducer: String,
    pub arg_bytes: Vec<u8>,
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
    pub table_updates: Vec<TableUpdateJson>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventJson {
    pub timestamp: u64,
    pub status: String,          // committed, failed
    pub caller_identity: String, // hex identity
    pub function_call: FunctionCallJson,
    pub energy_quanta_used: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionUpdateJson {
    pub event: EventJson,
    pub subscription_update: SubscriptionUpdateJson,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StmtResultJson {
    pub schema: TupleDef,
    pub rows: Vec<Vec<TypeValue>>,
}
