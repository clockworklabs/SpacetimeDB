use anyhow::{anyhow, Context, Result};
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::config::ConnectionConfig;
use crate::metrics_module_bindings::*;
use tokio::sync::oneshot;

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

pub async fn connect_metrics_module_async(config: &ConnectionConfig) -> Result<DbConnection> {
    let (ready_tx, ready_rx) = oneshot::channel();
    let ready_tx = Arc::new(Mutex::new(Some(ready_tx)));
    let success_tx = Arc::clone(&ready_tx);
    let error_tx = Arc::clone(&ready_tx);
    let database_identity = format!("{}-metrics", config.database_prefix);
    log::info!("connecting to metrics database {} at {}", database_identity, config.uri);
    let mut builder = DbConnection::builder()
        .with_uri(config.uri.clone())
        .with_database_name(database_identity)
        .with_confirmed_reads(config.confirmed_reads)
        .on_connect(move |_, _, _| {
            if let Some(tx) = success_tx.lock().expect("ready mutex poisoned").take() {
                let _ = tx.send(Ok::<(), anyhow::Error>(()));
            }
        })
        .on_connect_error(move |_, error| {
            if let Some(tx) = error_tx.lock().expect("ready mutex poisoned").take() {
                let _ = tx.send(Err(anyhow!("connection failed: {error}")));
            }
        });

    if let Some(token) = &config.token {
        builder = builder.with_token(Some(token.clone()));
    }

    let conn = builder.build().context("failed to build database connection")?;
    conn.run_threaded();
    tokio::time::timeout(Duration::from_secs(config.timeout_secs), ready_rx)
        .await
        .context("timed out waiting for connection")?
        .map_err(|_| anyhow!("metrics connection readiness callback dropped"))??;

    log::info!("metrics database connected");
    Ok(conn)
}
