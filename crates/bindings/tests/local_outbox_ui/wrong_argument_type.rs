#[spacetimedb::reducer]
pub fn receive_ping(_: &spacetimedb::ReducerContext, body: String) {
    let _ = body;
}

#[spacetimedb::table(accessor = wrong_argument_type, outbox(local = crate::receive_ping))]
pub struct WrongArgumentType {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[target]
    target: spacetimedb::Identity,
    #[param(name = body)]
    payload: u64,
}

fn main() {}
