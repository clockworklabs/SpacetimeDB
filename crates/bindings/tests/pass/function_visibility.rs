#![deny(warnings)]

use spacetimedb::rt::{FnInfo, FunctionVisibility};
use spacetimedb::{ProcedureContext, ReducerContext};

#[spacetimedb::reducer(internal)]
pub fn internal_reducer(_ctx: &ReducerContext) {}

#[spacetimedb::reducer(private)]
fn private_reducer(_ctx: &ReducerContext) {}

#[spacetimedb::reducer(public)]
fn public_reducer(_ctx: &ReducerContext) {}

#[spacetimedb::reducer(init, internal)]
fn initialize(_ctx: &ReducerContext) {}

#[spacetimedb::procedure(internal)]
fn internal_procedure(_ctx: &mut ProcedureContext) -> u64 {
    0
}

#[spacetimedb::procedure(private)]
fn private_procedure(_ctx: &mut ProcedureContext) -> u64 {
    0
}

#[spacetimedb::procedure(public)]
fn public_procedure(_ctx: &mut ProcedureContext) -> u64 {
    0
}

fn main() {
    assert!(matches!(
        internal_reducer::DECLARED_VISIBILITY,
        Some(FunctionVisibility::Internal)
    ));
    assert!(matches!(
        private_reducer::DECLARED_VISIBILITY,
        Some(FunctionVisibility::Private)
    ));
    assert!(matches!(
        public_reducer::DECLARED_VISIBILITY,
        Some(FunctionVisibility::ClientCallable)
    ));
    assert!(matches!(
        initialize::DECLARED_VISIBILITY,
        Some(FunctionVisibility::Internal)
    ));
    assert!(matches!(
        internal_procedure::DECLARED_VISIBILITY,
        Some(FunctionVisibility::Internal)
    ));
    assert!(matches!(
        private_procedure::DECLARED_VISIBILITY,
        Some(FunctionVisibility::Private)
    ));
    assert!(matches!(
        public_procedure::DECLARED_VISIBILITY,
        Some(FunctionVisibility::ClientCallable)
    ));
}
