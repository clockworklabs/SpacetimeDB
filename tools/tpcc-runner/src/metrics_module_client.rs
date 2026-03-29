use anyhow::{anyhow, Context, Result};
use std::sync::mpsc::sync_channel;
use std::time::Duration;

use crate::config::ConnectionConfig;
use crate::metrics_module_bindings::*;

pub fn connect_metrics_module(config: &ConnectionConfig) -> Result<DbConnection> {
    let (ready_tx, ready_rx) = sync_channel(1);
    let success_tx = ready_tx.clone();
    let error_tx = ready_tx;
    let database_identity = format!("{}-metrics", config.database_prefix);
    let mut builder = DbConnection::builder()
        .with_uri(config.uri.clone())
        .with_database_name(database_identity)
        .with_confirmed_reads(config.confirmed_reads)
        .on_connect(move |_, _, _| {
            let _ = success_tx.send(Ok::<(), anyhow::Error>(()));
        })
        .on_connect_error(move |_, error| {
            let _ = error_tx.send(Err(anyhow!("connection failed: {error}")));
        });

    if let Some(token) = &config.token {
        builder = builder.with_token(Some(token.clone()));
    }

    let conn = builder.build().context("failed to build database connection")?;
    conn.run_threaded();
    ready_rx
        .recv_timeout(Duration::from_secs(config.timeout_secs))
        .context("timed out waiting for connection")??;

    Ok(conn)
}
