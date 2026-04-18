use spacetimedb::http::{handler, router, Request, Response, Router};
use spacetimedb::{table, HandlerContext, ProcedureContext, Table};

#[handler]
fn handler_no_args() -> Response {
    todo!()
}

#[handler]
fn handler_immutable_ctx(_ctx: &HandlerContext, _req: Request) -> Response {
    todo!()
}

#[handler]
fn handler_wrong_ctx(_ctx: &mut ProcedureContext, _req: Request) -> Response {
    todo!()
}

#[handler]
fn handler_no_request_arg(_ctx: &mut HandlerContext) -> Response {
    todo!()
}

#[handler]
fn handler_wrong_request_arg_type(_ctx: &mut HandlerContext, _req: u32) -> Response {
    todo!()
}

#[handler]
fn handler_no_return_type(_ctx: &mut HandlerContext, _req: Request) {
    todo!()
}

#[handler]
fn handler_wrong_return_type(_ctx: &mut HandlerContext, _req: Request) -> u32 {
    todo!()
}

#[handler]
fn handler_no_sender(ctx: &mut HandlerContext, _req: Request) -> Response {
    let _sender = ctx.sender();
    let _conn_id = ctx.connection_id();
    todo!()
}

#[table(accessor = test_table)]
struct TestTable {
    data: u32,
}

#[handler]
fn handler_no_db(ctx: &mut HandlerContext, _req: Request) -> Response {
    let _rows = ctx.db.test_table().iter();
    todo!()
}

#[router]
static ROUTER_NOT_A_FUNCTION: Router = Router::new();

#[router]
fn router_fn_with_args(ctx: &mut HandlerContext) -> Router {
    todo!()
}

#[router]
fn router_fn_wrong_return_type() -> u32 {
    todo!()
}
