use serde::{Deserialize, Serialize};
use spacetimedb_lib::{ProductType, ProductValue};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StmtStatsJson {
    pub rows_inserted: u64,
    pub rows_deleted: u64,
    pub rows_updated: u64,
    pub rows_scanned: u64,
}

// Sync with spacetimedb_cli::api::StmtResultJson
#[derive(Debug, Clone, Serialize)]
pub struct StmtResultJson {
    pub schema: ProductType,
    pub rows: Vec<ProductValue>,
    pub total_duration_micros: u64,
    pub stats: StmtStatsJson,
}
