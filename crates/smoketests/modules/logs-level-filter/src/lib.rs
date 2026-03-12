use spacetimedb::ReducerContext;

#[spacetimedb::reducer]
pub fn log_all_levels(_ctx: &ReducerContext) {
    log::trace!("msg-trace");
    log::debug!("msg-debug");
    log::info!("msg-info");
    log::warn!("msg-warn");
    log::error!("msg-error");
}
