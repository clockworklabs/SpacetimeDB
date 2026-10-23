use spacetimedb::{log, ProcedureContext, ReducerContext};

#[spacetimedb::reducer]
pub fn say_reducer_0(_ctx: &ReducerContext) {
    log::info!("Hello from reducer 0!");
}

#[spacetimedb::procedure]
pub fn say_procedure_0(_ctx: &mut ProcedureContext) {
    log::info!("Hello from procedure 0!");
}

#[spacetimedb::reducer]
pub fn say_reducer_1(_ctx: &ReducerContext) {
    log::info!("Hello from reducer 1!");
}

#[spacetimedb::procedure]
pub fn say_procedure_1(_ctx: &mut ProcedureContext) {
    log::info!("Hello from procedure 1!");
}

#[spacetimedb::reducer]
pub fn say_reducer_2(_ctx: &ReducerContext) {
    log::info!("Hello from reducer 2!");
}

#[spacetimedb::procedure]
pub fn say_procedure_2(_ctx: &mut ProcedureContext) {
    log::info!("Hello from procedure 2!");
}

#[spacetimedb::reducer]
pub fn say_reducer_3(_ctx: &ReducerContext) {
    log::info!("Hello from reducer 3!");
}

#[spacetimedb::procedure]
pub fn say_procedure_3(_ctx: &mut ProcedureContext) {
    log::info!("Hello from procedure 3!");
}

#[spacetimedb::reducer]
pub fn say_reducer_4(_ctx: &ReducerContext) {
    log::info!("Hello from reducer 4!");
}

#[spacetimedb::procedure]
pub fn say_procedure_4(_ctx: &mut ProcedureContext) {
    log::info!("Hello from procedure 4!");
}

#[spacetimedb::reducer]
pub fn say_reducer_5(_ctx: &ReducerContext) {
    log::info!("Hello from reducer 5!");
}

#[spacetimedb::procedure]
pub fn say_procedure_5(_ctx: &mut ProcedureContext) {
    log::info!("Hello from procedure 5!");
}

#[spacetimedb::reducer]
pub fn say_reducer_6(_ctx: &ReducerContext) {
    log::info!("Hello from reducer 6!");
}

#[spacetimedb::procedure]
pub fn say_procedure_6(_ctx: &mut ProcedureContext) {
    log::info!("Hello from procedure 6!");
}

#[spacetimedb::reducer]
pub fn say_reducer_7(_ctx: &ReducerContext) {
    log::info!("Hello from reducer 7!");
}

#[spacetimedb::procedure]
pub fn say_procedure_7(_ctx: &mut ProcedureContext) {
    log::info!("Hello from procedure 7!");
}

#[spacetimedb::reducer]
pub fn say_reducer_8(_ctx: &ReducerContext) {
    log::info!("Hello from reducer 8!");
}

#[spacetimedb::procedure]
pub fn say_procedure_8(_ctx: &mut ProcedureContext) {
    log::info!("Hello from procedure 8!");
}

#[spacetimedb::reducer]
pub fn say_reducer_9(_ctx: &ReducerContext) {
    log::info!("Hello from reducer 9!");
}

#[spacetimedb::procedure]
pub fn say_procedure_9(_ctx: &mut ProcedureContext) {
    log::info!("Hello from procedure 9!");
}

#[spacetimedb::reducer]
pub fn say_reducer_10(_ctx: &ReducerContext) {
    log::info!("Hello from reducer 10!");
}

#[spacetimedb::procedure]
pub fn say_procedure_10(_ctx: &mut ProcedureContext) {
    log::info!("Hello from procedure 10!");
}
