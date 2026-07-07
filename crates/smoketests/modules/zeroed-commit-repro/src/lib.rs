use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(accessor = payload, public)]
pub struct Payload {
    #[primary_key]
    id: u64,
    bytes: Vec<u8>,
}

#[spacetimedb::table(accessor = control)]
pub struct Control {
    #[primary_key]
    id: u8,
    disconnect_panics: bool,
    connect_panics: bool,
}

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.control().insert(Control {
        id: 0,
        disconnect_panics: false,
        connect_panics: false,
    });
}

#[spacetimedb::reducer]
pub fn set_disconnect_panics(ctx: &ReducerContext, enabled: bool) {
    set_control(ctx, Some(enabled), None);
}

#[spacetimedb::reducer]
pub fn set_connect_panics(ctx: &ReducerContext, enabled: bool) {
    set_control(ctx, None, Some(enabled));
}

#[spacetimedb::reducer]
pub fn write_payload(ctx: &ReducerContext, id: u64, len: u32) {
    let mut bytes = Vec::with_capacity(len as usize);
    for ix in 0..len {
        bytes.push(((id.wrapping_add(ix as u64)) & 0xff) as u8);
    }

    let row = Payload { id, bytes };
    if ctx.db.payload().id().find(&id).is_some() {
        ctx.db.payload().id().update(row);
    } else {
        ctx.db.payload().insert(row);
    }
}

#[spacetimedb::reducer]
pub fn delete_payload(ctx: &ReducerContext, id: u64) {
    ctx.db.payload().id().delete(&id);
}

#[spacetimedb::reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    if control(ctx).connect_panics {
        panic!("zeroed-commit-repro connect panic armed");
    }
    log::info!("zeroed-commit-repro client connected");
}

#[spacetimedb::reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    if control(ctx).disconnect_panics {
        panic!("zeroed-commit-repro disconnect panic armed");
    }
    log::info!("zeroed-commit-repro client disconnected");
}

fn control(ctx: &ReducerContext) -> Control {
    ctx.db
        .control()
        .id()
        .find(&0)
        .expect("zeroed-commit-repro control row missing")
}

fn set_control(ctx: &ReducerContext, disconnect_panics: Option<bool>, connect_panics: Option<bool>) {
    let mut row = control(ctx);
    if let Some(enabled) = disconnect_panics {
        row.disconnect_panics = enabled;
    }
    if let Some(enabled) = connect_panics {
        row.connect_panics = enabled;
    }
    ctx.db.control().id().update(row);
}
