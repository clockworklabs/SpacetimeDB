use spacetimedb::{
    reducer, remote_reducer::call_reducer_on_db, table, DeserializeOwned, Identity, ReducerContext, Serialize,
    SpacetimeType, Table,
};
use spacetimedb_sats::bsatn;


/// For warehouses not managed by this database, stores the [`Identity`] of the remote database which manages that warehouse.
///
/// Will not have a row present for a warehouse managed by the local database.
#[table(accessor = remote_warehouse)]
pub struct RemoteWarehouse {
    #[primary_key]
    pub w_id: u32,
    pub remote_database_home: Identity,
}

#[reducer]
fn load_remote_warehouses(ctx: &ReducerContext, rows: Vec<RemoteWarehouse>) -> Result<(), String> {
    replace_remote_warehouses(ctx, rows)
}

pub fn replace_remote_warehouses(ctx: &ReducerContext, rows: Vec<RemoteWarehouse>) -> Result<(), String> {
    clear_remote_warehouses(ctx);
    for row in rows {
        ctx.db.remote_warehouse().try_insert(row)?;
    }
    Ok(())
}

pub fn clear_remote_warehouses(ctx: &ReducerContext) {
    for row in ctx.db.remote_warehouse().iter() {
        ctx.db.remote_warehouse().delete(row);
    }
}

pub fn remote_warehouse_home(ctx: &ReducerContext, warehouse_id: u32) -> Option<Identity> {
    ctx.db
        .remote_warehouse()
        .w_id()
        .find(warehouse_id)
        .map(|row| row.remote_database_home)
}

pub fn call_remote_reducer<Args, Output>(
    _ctx: &ReducerContext,
    database_ident: Identity,
    reducer_name: &str,
    args: &Args,
) -> Result<Output, String>
where
    Args: SpacetimeType + Serialize,
    Output: SpacetimeType + DeserializeOwned,
{
    let args = bsatn::to_vec(args).map_err(|e| {
        format!("Failed to BSATN-serialize args for remote reducer {reducer_name} on database {database_ident}: {e}")
    })?;
    let out = call_reducer_on_db(database_ident, reducer_name, &args)
        .map_err(|e| format!("Failed to call remote reducer {reducer_name} on database {database_ident}: {e}"))?;
    bsatn::from_slice(&out).map_err(|e| {
        format!(
            "Failed to BSATN-deserialize result from remote reducer {reducer_name} on database {database_ident}: {e}"
        )
    })
}
