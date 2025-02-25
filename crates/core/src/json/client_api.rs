use serde::Serialize;
use spacetimedb_lib::{ProductType, ProductValue};

#[derive(Debug, Clone, Serialize)]
pub struct StmtResultJson {
    pub schema: ProductType,
    pub rows: Vec<ProductValue>,
    pub total_duration_micros: u64,
}
