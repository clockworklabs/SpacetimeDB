use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = command_result, public)]
pub struct CommandResult {
    #[primary_key]
    pub request_id: String,
    pub success: bool,
    pub message: String,
}

#[reducer]
pub fn run_command(ctx: &ReducerContext, request_id: String, value: i32) {
    ctx.db.command_result().insert(CommandResult {
        request_id,
        success: true,
        message: format!("value={value}"),
    });
}
