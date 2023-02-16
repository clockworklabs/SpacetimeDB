use serde::{Deserialize, Serialize};
use spacetimedb_lib::TupleDef;
use spacetimedb_lib::TypeValue;

use serde_with::serde_as;

struct Sats;

impl<T: spacetimedb_lib::ser::Serialize> serde_with::SerializeAs<T> for Sats {
    fn serialize_as<S>(source: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        source
            .serialize(spacetimedb_lib::ser::serde::SerdeSerializer::new(serializer))
            .map_err(|e| e.0)
    }
}

impl<'de, T: spacetimedb_lib::de::Deserialize<'de>> serde_with::DeserializeAs<'de, T> for Sats {
    fn deserialize_as<D>(deserializer: D) -> Result<T, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        T::deserialize(spacetimedb_lib::de::serde::SerdeDeserializer::new(deserializer)).map_err(|e| e.0)
    }
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
pub struct TableUpdateJson {
    pub table_id: u32,
    pub table_name: String,
    pub table_row_operations: Vec<TableRowOperationJson>,
}

#[serde_as]
#[derive(Debug, Clone, Serialize)]
pub struct TableRowOperationJson {
    pub op: String,
    pub row_pk: String,
    #[serde_as(as = "Vec<Sats>")]
    pub row: Vec<TypeValue>,
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
pub struct TransactionUpdateJson {
    pub event: EventJson,
    pub subscription_update: SubscriptionUpdateJson,
}

#[serde_as]
#[derive(Debug, Clone, Serialize)]
pub struct StmtResultJson {
    pub schema: TupleDef,
    #[serde_as(as = "Vec<Vec<Sats>>")]
    pub rows: Vec<Vec<TypeValue>>,
}
