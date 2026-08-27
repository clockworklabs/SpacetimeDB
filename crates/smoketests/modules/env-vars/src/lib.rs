use spacetimedb::ReducerContext;

#[spacetimedb::reducer]
fn read_env(ctx: &ReducerContext, key: String) {
    match ctx.env.get(&key) {
        Some(value) => log::info!("env: {key}={value}"),
        None => log::info!("env: {key} is unset"),
    }
}

/// Bypasses the SDK bindings to verify that the host hides `st_env` from modules.
#[spacetimedb::reducer]
fn probe_st_env(_ctx: &ReducerContext) {
    match spacetimedb::sys::table_id_from_name("st_env") {
        Ok(_) => log::info!("probe: resolved st_env"),
        Err(_) => log::info!("probe: st_env not found"),
    }
}
