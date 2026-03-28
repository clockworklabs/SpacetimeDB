use std::time::Duration;

use http::Request;
use spacetimedb::{
    http::Timeout, reducer, table, Identity, ProcedureContext, ReducerContext, Serialize, Table, TimeDuration,
    TxContext,
};
use spacetimedb_sats::bsatn;

use crate::WarehouseId;

#[table(accessor = spacetimedb_uri)]
struct SpacetimeDbUri {
    uri: String,
}

#[reducer]
fn set_spacetimedb_uri(ctx: &ReducerContext, uri: String) {
    for row in ctx.db.spacetimedb_uri().iter() {
        ctx.db.spacetimedb_uri().delete(row);
    }
    ctx.db.spacetimedb_uri().insert(SpacetimeDbUri { uri });
}

pub fn get_spacetimedb_uri(tx: &TxContext) -> String {
    tx.db.spacetimedb_uri().iter().next().unwrap().uri
}

/// For warehouses not managed by this database, stores the [`Identity`] of the remote database which manages that warehouse.
///
/// Will not have a row present for a warehouse managed by the local database.
#[table(accessor = remote_warehouse)]
pub struct RemoteWarehouse {
    #[primary_key]
    pub w_id: WarehouseId,
    pub remote_database_home: Identity,
}

#[reducer]
fn load_remote_warehouses(ctx: &ReducerContext, rows: Vec<RemoteWarehouse>) -> Result<(), String> {
    for row in rows {
        ctx.db.remote_warehouse().try_insert(row)?;
    }
    Ok(())
}

pub fn reset_remote_warehouses(ctx: &ReducerContext) {
    for row in ctx.db.remote_warehouse().iter() {
        ctx.db.remote_warehouse().delete(row);
    }
}

pub fn remote_warehouse_home(ctx: &ReducerContext, warehouse_id: WarehouseId) -> Option<Identity> {
    ctx.db
        .remote_warehouse()
        .w_id()
        .find(warehouse_id)
        .map(|row| row.remote_database_home)
}

pub fn call_remote_function(
    ctx: &mut ProcedureContext,
    spacetimedb_uri: &str,
    database_ident: Identity,
    function_name: &str,
    arguments: impl Serialize,
) -> Result<spacetimedb::http::Body, String> {
    let request = Request::builder()
        .uri(format!(
            "{spacetimedb_uri}/v1/database/{database_ident}/call/{function_name}"
        ))
        .method("POST")
        .header("Content-Type", "application/octet-stream")
        // This absurdly long timeout will be clamped by the host to 3 minutes.
        .extension(Timeout::from(TimeDuration::from_duration(Duration::from_hours(1))))
        // TODO(auth): include a token.
        .body(bsatn::to_vec(&arguments).map_err(|e| format!("Failed to BSATN-serialize arguments: {e}"))?)
        .map_err(|e| format!("Error constructing `Request`: {e}"))?;
    log::debug!("Sending remote request to run {function_name} on {spacetimedb_uri} / {database_ident}");
    match ctx.http.send(request) {
        Err(e) => {
            let msg = format!("Error sending request to remote database {database_ident} at URI {spacetimedb_uri} to call {function_name}: {e}");
            log::error!("{}", msg);
            Err(msg)
        }
        Ok(response) if response.status() != http::status::StatusCode::OK => {
            let msg = format!("Got non-200 response code {} from request to remote database {database_ident} at URI {spacetimedb_uri} when calling {function_name}: {}", response.status(), response.into_body().into_string_lossy());
            log::error!("{}", msg);
            Err(msg)
        }
        Ok(response) => {
            log::debug!(
                "Got successful response from {spacetimedb_uri} / {database_ident} when running {function_name}"
            );
            Ok(response.into_body())
        }
    }
}
