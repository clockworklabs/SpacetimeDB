// Interface fixture used to generate the supplied client bindings.
use spacetimedb::ReducerContext;
#[spacetimedb::table(accessor = counter, public)]
pub struct Counter {
    #[primary_key]
    id: u32,
    value: i32,
}
#[spacetimedb::reducer(init)]
pub fn init(_ctx: &ReducerContext) {}
#[spacetimedb::reducer]
pub fn add(_ctx: &ReducerContext, delta: i32) -> Result<(), String> {
    let _ = delta;
    Ok(())
}
