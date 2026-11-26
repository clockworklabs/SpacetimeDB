// Interface fixture used to generate the supplied module bindings.
#[spacetimedb::reducer]
pub fn receive(_ctx: &spacetimedb::ReducerContext, body: String, reject: bool) {
    let _ = (body, reject);
}
