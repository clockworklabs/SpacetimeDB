use anyhow::Result;

use crate::client::ModuleClient;
use crate::config::StatusConfig;
use crate::topology::lookup_database_identities;

pub async fn run(config: StatusConfig) -> Result<()> {
    log::info!(
        "Checking tpcc load state for {} databases on {}",
        config.num_databases,
        config.connection.uri
    );

    let identities = lookup_database_identities(&config.connection, config.num_databases).await?;

    for (database_number, database_identity) in (0..config.num_databases).zip(identities.into_iter()) {
        let database_name = format!("{}-{}", config.connection.database_prefix, database_number);
        let mut client = ModuleClient::connect(&config.connection, database_identity)?;
        client.subscribe_load_state()?;

        if let Some(state) = client.load_state() {
            println!(
                "{database_name} identity={database_identity} status={:?} phase={:?} chunks_completed={} rows_inserted={} next=({},{},{},{}) started_at={:?} updated_at={:?} completed_at={:?} last_error={:?}",
                state.status,
                state.phase,
                state.chunks_completed,
                state.rows_inserted,
                state.next_warehouse_id,
                state.next_district_id,
                state.next_item_id,
                state.next_order_id,
                state.started_at,
                state.updated_at,
                state.completed_at,
                state.last_error,
            );
        } else {
            println!("{database_name} identity={database_identity} load_state=missing");
        }

        client.shutdown();
    }

    Ok(())
}
