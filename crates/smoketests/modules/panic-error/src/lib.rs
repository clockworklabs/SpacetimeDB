use spacetimedb::ReducerContext;

#[spacetimedb::reducer]
fn fail(_ctx: &ReducerContext) -> Result<(), String> {
    Err("oopsie :(".into())
}
