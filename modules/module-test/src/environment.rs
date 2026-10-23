//! Optional declarations keep this general-purpose test module publishable
//! without configuration, just like the C#, C++, and TypeScript examples.

use spacetimedb::{ProcedureContext, ReducerContext};

#[spacetimedb::env]
pub struct Env {
    pub MISSING: Option<String>,
    pub EMPTY: Option<String>,
    pub UTF8: Option<String>,
    pub NUL: Option<String>,
    pub MAXIMUM: Option<String>,
}

#[spacetimedb::reducer]
pub fn expect_environment(ctx: &ReducerContext, key: String, expected: Option<String>) {
    assert_eq!(ctx.env.EMPTY(), ctx.env.get("EMPTY"));
    assert_eq!(ctx.env.get(&key), expected);
}

#[spacetimedb::procedure]
pub fn read_environment(ctx: &mut ProcedureContext, key: String) -> Option<String> {
    let outside = ctx.env.get(&key);
    ctx.with_tx(|tx| assert_eq!(tx.env.get(&key), outside));
    outside
}
