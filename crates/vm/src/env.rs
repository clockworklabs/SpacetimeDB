//! The different `environments` that support the runtime/compilation storage
//! of `idents`, `functions`, `lambdas`, etc.
use std::collections::HashMap;

use crate::expr::{Code, FunctionId};
use crate::functions::{FunDef, FunVM, FunVm, Lambda};
use crate::operator::Op;
use crate::types::Ty;

#[derive(Debug, Clone)]
pub struct EnvArena<T> {
    env: Vec<T>,
    pub(crate) names: HashMap<String, usize>,
}

impl<T> EnvArena<T> {
    pub fn new() -> Self {
        Self {
            env: Vec::new(),
            names: HashMap::new(),
        }
    }

    pub fn next_id(&self) -> usize {
        self.env.len()
    }

    pub fn add<N: Into<String>>(&mut self, name: N, f: T) -> usize {
        let idx = self.env.len();
        self.env.push(f);
        self.names.insert(name.into(), idx);
        idx
    }

    pub fn update<N: Into<String>>(&mut self, name: N, f: T) -> bool {
        if let Some(id) = self.get_id(&(name.into())) {
            self.env[id] = f;
            true
        } else {
            false
        }
    }

    pub fn get(&self, key: usize) -> Option<&T> {
        self.env.get(key)
    }

    pub fn get_id(&self, name: &str) -> Option<usize> {
        self.names.get(name).cloned()
    }

    pub fn get_by_name(&self, name: &str) -> Option<&T> {
        if let Some(k) = self.names.get(name) {
            self.get(*k)
        } else {
            None
        }
    }
}

impl<T> Default for EnvArena<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct EnvTy {
    env: HashMap<String, Ty>,
}

impl EnvTy {
    pub fn new() -> Self {
        Self {
            env: Default::default(),
        }
    }

    pub(crate) fn add(&mut self, name: &str, v: Ty) {
        self.env.insert(name.to_string(), v);
    }

    pub(crate) fn get(&self, key: &str) -> Option<&Ty> {
        self.env.get(key)
    }
}

impl Default for EnvTy {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct EnvFun {
    env: EnvArena<FunVm>,
    pub(crate) ops: HashMap<Op, FunctionId>,
}

impl EnvFun {
    pub fn new() -> Self {
        Self {
            env: EnvArena::default(),
            ops: Default::default(),
        }
    }

    pub fn add<'a, T: Into<&'a str>>(&mut self, name: T, f: Box<dyn FunVM>) -> FunctionId {
        let idx = self.env.next_id();
        let name = name.into();
        let f = FunVm::new(name, idx, f);
        self.env.add(name, f) as FunctionId
    }

    pub(crate) fn get(&self, key: FunctionId) -> Option<&FunVm> {
        self.env.get(key)
    }

    pub(crate) fn get_by_name(&mut self, name: &str) -> Option<&FunVm> {
        self.env.get_by_name(name)
    }

    pub(crate) fn get_function_id_op(&self, op: Op) -> FunctionId {
        *self.ops.get(&op).unwrap_or_else(|| panic!("Op {:?} is not loaded", op))
    }

    pub fn get_op(&self, op: Op) -> &FunVm {
        let id = self.get_function_id_op(op);
        self.get(id).unwrap()
    }
}

impl Default for EnvFun {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct EnvIdent {
    env: EnvArena<Code>,
}

impl EnvIdent {
    pub fn new() -> Self {
        Self {
            env: EnvArena::default(),
        }
    }

    pub(crate) fn add(&mut self, name: &str, v: Code) {
        self.env.add(name, v);
    }

    pub(crate) fn get_by_name(&self, key: &str) -> Option<&Code> {
        self.env.get_by_name(key)
    }
}

impl Default for EnvIdent {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct EnvLambda {
    env: EnvArena<Lambda>,
}

impl EnvLambda {
    pub fn new() -> Self {
        Self {
            env: Default::default(),
        }
    }

    pub(crate) fn add(&mut self, f: FunDef, body: Code) {
        self.env.add(f.name.clone(), Lambda { head: f, body });
    }

    pub(crate) fn update(&mut self, f: FunDef, body: Code) {
        self.env.update(f.name.clone(), Lambda { head: f, body });
    }

    pub(crate) fn get_id(&self, key: &str) -> Option<usize> {
        self.env.get_id(key)
    }

    pub(crate) fn get(&self, key: usize) -> Option<&Lambda> {
        self.env.get(key)
    }
}

impl Default for EnvLambda {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct EnvDb {
    pub functions: EnvFun,
    pub(crate) lambdas: EnvLambda,
    pub idents: EnvIdent,
    pub(crate) ty: EnvTy,
    pub(crate) child: Vec<EnvDb>,
}

impl Default for EnvDb {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvDb {
    pub fn new() -> Self {
        Self {
            functions: EnvFun::new(),
            lambdas: EnvLambda::new(),
            idents: EnvIdent::new(),
            ty: EnvTy::new(),
            child: Vec::new(),
        }
    }

    pub fn push_scope(&mut self) {
        self.child.push(Self::new());
    }
    pub fn pop_scope(&mut self) {
        self.child.pop();
    }
}
