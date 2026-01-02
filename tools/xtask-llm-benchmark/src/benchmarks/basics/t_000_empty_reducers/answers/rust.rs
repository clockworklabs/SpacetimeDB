use spacetimedb::{reducer, ReducerContext};

#[reducer]
pub fn empty_reducer_no_args(ctx: &ReducerContext) -> Result<(), String> {
    Ok(())
}

#[reducer]
pub fn empty_reducer_with_int(ctx: &ReducerContext, count: i32) -> Result<(), String> {
    Ok(())
}

#[reducer]
pub fn empty_reducer_with_string(ctx: &ReducerContext, name: String) -> Result<(), String> {
    Ok(())
}

#[reducer]
pub fn empty_reducer_with_two_args(ctx: &ReducerContext, count: i32, name: String) -> Result<(), String> {
    Ok(())
}

#[reducer]
pub fn empty_reducer_with_three_args(
    ctx: &ReducerContext,
    active: bool,
    ratio: f32,
    label: String,
) -> Result<(), String> {
    Ok(())
}
