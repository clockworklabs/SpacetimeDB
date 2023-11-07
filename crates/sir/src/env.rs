//! The different `environments` that support the runtime/compilation storage
//! of `idents`, `functions`, `lambdas`, etc.
use crate::arena::Arena;
use crate::ast::Sir;
use crate::wasm::Wasm;

pub type EnvIdent = Arena<String, Sir>;

pub struct Env {
    wasm: Wasm,
    child: Vec<Env>,
}

impl Env {
    pub fn new() -> Self {
        Self {
            wasm: Wasm::new(),
            child: vec![],
        }
    }
}
