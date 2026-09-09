use spacetimedb::{AnonymousViewContext, ProcedureContext, ReducerContext, SpacetimeType};
use std::sync::atomic::{AtomicBool, Ordering};

static VIEW_TRAP_ENTERED: AtomicBool = AtomicBool::new(false);

type RequiredString = String;
type OptionalString = Option<RequiredString>;

#[spacetimedb::env]
pub struct Env {
    pub REQUIRED: RequiredString,
    #[env(values("ready", "other"))]
    pub MODE: String,
    pub MISSING: OptionalString,
    pub EMPTY: Option<String>,
    pub UTF8: Option<String>,
    pub NUL: Option<String>,
    pub MAXIMUM: Option<String>,
    pub HANDLER: Option<String>,
    pub WATCHED: Option<String>,
    pub LIMIT: Option<String>,
}

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    assert_eq!(ctx.env.REQUIRED(), "initial-required");
    assert_eq!(ctx.env.MODE(), "ready");
}

#[spacetimedb::reducer]
pub fn expect_environment(ctx: &ReducerContext, key: String, expected: Option<String>) {
    assert!(
        !VIEW_TRAP_ENTERED.load(Ordering::Relaxed),
        "trapped Wasm view instance was reused"
    );
    assert_eq!(ctx.env.get(&key), expected);
    assert_eq!(ctx.as_read_only().env.get(&key), expected);
    assert_eq!(ctx.as_anonymous_read_only().env.get(&key), expected);
}

#[spacetimedb::procedure]
pub fn read_environment(ctx: &mut ProcedureContext, key: String) -> Option<String> {
    let outside = ctx.env.get(&key);
    ctx.with_tx(|tx| assert_eq!(tx.env.get(&key), outside));
    outside
}

/// The test server holds this request while the database installs a new program.
#[spacetimedb::procedure]
pub fn read_environment_after_http(ctx: &mut ProcedureContext, url: String, explicit_tx: bool) -> Option<String> {
    let request = spacetimedb::http::Request::builder()
        .uri(url)
        .extension(spacetimedb::http::Timeout::from(spacetimedb::TimeDuration::from(
            std::time::Duration::from_secs(30),
        )))
        .body(())
        .unwrap();
    assert!(ctx.http.send(request).unwrap().status().is_success());
    if explicit_tx {
        ctx.with_tx(|tx| tx.env.REQUIRED().into())
    } else {
        ctx.env.REQUIRED().into()
    }
}

#[derive(SpacetimeType)]
pub struct EnvironmentValue {
    pub value: Option<String>,
}

#[spacetimedb::view(accessor = environment_value, public)]
pub fn environment_value(ctx: &AnonymousViewContext) -> Option<EnvironmentValue> {
    let value = ctx.env.WATCHED();
    assert_ne!(value.as_deref(), Some("fail-view"));
    Some(EnvironmentValue { value })
}

/// A Rust panic is a Wasm trap. The next main-instance reducer verifies that
/// the guest-global marker did not survive the failed SQL/subscription call.
#[spacetimedb::view(accessor = environment_trap, public)]
pub fn environment_trap(_ctx: &AnonymousViewContext) -> Option<EnvironmentValue> {
    VIEW_TRAP_ENTERED.store(true, Ordering::Relaxed);
    panic!("intentional environment view trap");
}

/// Hand-written ABI callers cannot retain unbounded host allocations.
#[spacetimedb::reducer]
pub fn bounded_environment_sources(_ctx: &ReducerContext) {
    use spacetimedb::sys::raw::{self, BytesSource};
    let mut sources = Vec::new();
    for i in 0..=256 {
        let mut source = BytesSource::INVALID;
        let status = unsafe { raw::env_get(b"LIMIT".as_ptr(), 5, &mut source) };
        if i == 256 {
            assert_eq!(status, 9); // NO_SPACE
        } else {
            assert_eq!(status, 0);
            assert!(source != BytesSource::INVALID);
            sources.push(source);
        }
    }
    let mut buffer = [0u8; 8192];
    let mut len = buffer.len();
    let status = unsafe { raw::bytes_source_read(sources[0], buffer.as_mut_ptr(), &mut len) };
    assert_eq!(status, -1);
    assert_eq!(len, buffer.len());
    let mut source = BytesSource::INVALID;
    assert_eq!(unsafe { raw::env_get(b"LIMIT".as_ptr(), 5, &mut source) }, 0);
    assert!(source != BytesSource::INVALID);
    // The remaining sources are released when this invocation ends.
}

#[spacetimedb::http::handler]
pub fn handler_environment(
    ctx: &mut spacetimedb::http::HandlerContext,
    _request: spacetimedb::http::Request,
) -> spacetimedb::http::Response {
    let outside = ctx.env.get("HANDLER");
    ctx.with_tx(|tx| assert_eq!(tx.env.get("HANDLER"), outside));
    spacetimedb::http::Response::new(spacetimedb::http::Body::from_bytes(outside.unwrap()))
}

#[spacetimedb::http::router]
pub fn router() -> spacetimedb::http::Router {
    spacetimedb::http::Router::new().get("/environment", handler_environment)
}
