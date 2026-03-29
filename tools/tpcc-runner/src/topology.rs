use anyhow::{bail, Result};
use reqwest::Client;
use spacetimedb_sdk::Identity;

use crate::config::{ConnectionConfig, DriverConfig, LoadConfig};

#[derive(Clone, Debug)]
pub struct DatabaseTopology {
    database_prefix: String,
    warehouses_per_database: u32,
    identities: Vec<Identity>,
}

impl DatabaseTopology {
    pub async fn for_load(config: &LoadConfig) -> Result<Self> {
        ensure_warehouses_per_database(config.warehouses_per_database)?;
        Ok(Self {
            database_prefix: config.connection.database_prefix.clone(),
            warehouses_per_database: config.warehouses_per_database,
            identities: lookup_database_identities(&config.connection, config.num_databases).await?,
        })
    }

    pub async fn for_driver(config: &DriverConfig) -> Result<Self> {
        ensure_warehouses_per_database(config.warehouses_per_database)?;
        Ok(Self {
            database_prefix: config.connection.database_prefix.clone(),
            warehouses_per_database: config.warehouses_per_database,
            identities: lookup_database_identities(
                &config.connection,
                required_database_count(config.warehouse_count, config.warehouses_per_database),
            )
            .await?,
        })
    }

    pub fn database_name(&self, database_number: u32) -> String {
        format!("{}-{}", self.database_prefix, database_number)
    }

    pub fn identity_for_database_number(&self, database_number: u32) -> Result<Identity> {
        self.identities
            .get(usize::try_from(database_number).expect("u32 fits usize"))
            .copied()
            .ok_or_else(|| {
            anyhow::anyhow!(
                "missing database identity for database {}",
                self.database_name(database_number)
            )
        })
    }

    pub fn database_number_for_warehouse(&self, warehouse_id: u32) -> Result<u32> {
        if warehouse_id == 0 {
            bail!("warehouse id must be positive");
        }
        Ok((warehouse_id - 1) / self.warehouses_per_database)
    }

    pub fn identity_for_warehouse(&self, warehouse_id: u32) -> Result<Identity> {
        let database_number = self.database_number_for_warehouse(warehouse_id)?;
        self.identity_for_database_number(database_number)
    }
}

pub fn required_database_count(warehouse_count: u32, warehouses_per_database: u32) -> u32 {
    warehouse_count.div_ceil(warehouses_per_database)
}

pub async fn lookup_database_identities(connection: &ConnectionConfig, num_databases: u32) -> Result<Vec<Identity>> {
    log::info!(
        "Looking up identities for {num_databases} at {} / {}-*",
        connection.uri,
        connection.database_prefix
    );
    let result = async {
        let client = Client::new();
        let mut identities = Vec::with_capacity(usize::try_from(num_databases).expect("u32 fits usize"));
        for database_number in 0..num_databases {
            let body = client
                .get(format!(
                    "{}/v1/database/{}-{}",
                    connection.uri, connection.database_prefix, database_number
                ))
                .send()
                .await?
                .error_for_status()?;
            let obj = match body.json::<serde_json::Value>().await? {
                serde_json::Value::Object(obj) => obj,
                els => bail!("expected object while resolving database identity, got {els:?}"),
            };
            let Some(db_ident) = obj.get("database_identity") else {
                bail!("missing database_identity in response {obj:?}");
            };
            let serde_json::Value::Object(ident_obj) = db_ident else {
                bail!("expected database_identity object, got {db_ident:?}");
            };
            let Some(ident_str) = ident_obj.get("__identity__") else {
                bail!("missing __identity__ in response {ident_obj:?}");
            };
            let serde_json::Value::String(ident_str) = ident_str else {
                bail!("expected __identity__ string, got {ident_str:?}");
            };
            identities.push(Identity::from_hex(ident_str)?);
        }
        Ok(identities)
    }
    .await;

    match &result {
        Ok(_) => log::info!("Successfully got database identities"),
        Err(e) => log::error!("Failed to get database identities: {e}"),
    }

    result
}

fn ensure_warehouses_per_database(warehouses_per_database: u32) -> Result<()> {
    if warehouses_per_database == 0 {
        bail!("warehouses_per_database must be positive");
    }
    Ok(())
}
