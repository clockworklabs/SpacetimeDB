//! Definition for a `Program` to run code.
//!
//! It carries an [EnvDb] with the functions, idents, types.
use spacetimedb_sats::relation::{MemTable, RelIter, Relation, Table};
use std::collections::HashMap;

use crate::env::EnvDb;
use crate::errors::ErrorVm;
use crate::eval::{build_query, IterRows};
use crate::expr::{Code, FunctionId, QueryCode};
use crate::functions::FunDef;
use crate::operator::*;
use crate::ops::logic;
use crate::ops::math;
use crate::rel_ops::RelOps;

/// A trait to allow split the execution of `programs` to allow executing
/// `queries` that take in account each `program` state/enviroment.
///
/// In concrete, it allows to run queries that run on the `SpaceTimeDb` engine.
///
/// It could also permite run queries backed by different engines, like in `MySql`.
pub trait ProgramVm {
    /// Load the in-built functions that define the operators of the VM,
    /// like `+`, `and`, `==`, etc.
    fn load_ops(env: &mut EnvDb) {
        let mut ops: HashMap<Op, FunctionId> = HashMap::new();

        ops.insert(OpCmp::Eq.into(), env.functions.add(OpCmp::Eq, Box::new(logic::eq)));
        ops.insert(
            OpCmp::NotEq.into(),
            env.functions.add(OpCmp::NotEq, Box::new(logic::not_eq)),
        );
        ops.insert(
            OpCmp::Less.into(),
            env.functions.add(OpCmp::Less, Box::new(logic::less)),
        );
        ops.insert(
            OpCmp::LessThan.into(),
            env.functions.add(OpCmp::LessThan, Box::new(logic::less_than)),
        );
        ops.insert(
            OpCmp::Greater.into(),
            env.functions.add(OpCmp::Greater, Box::new(logic::greater)),
        );
        ops.insert(
            OpCmp::GreaterThan.into(),
            env.functions.add(OpCmp::GreaterThan, Box::new(logic::greater_than)),
        );
        ops.insert(
            OpUnary::Not.into(),
            env.functions.add(OpUnary::Not, Box::new(logic::not)),
        );
        ops.insert(
            OpLogic::And.into(),
            env.functions.add(OpLogic::And, Box::new(logic::and)),
        );
        ops.insert(OpLogic::Or.into(), env.functions.add(OpLogic::Or, Box::new(logic::or)));

        ops.insert(OpMath::Add.into(), env.functions.add(OpMath::Add, Box::new(math::add)));
        ops.insert(
            OpMath::Minus.into(),
            env.functions.add(OpMath::Minus, Box::new(math::minus)),
        );
        ops.insert(OpMath::Mul.into(), env.functions.add(OpMath::Mul, Box::new(math::mul)));
        ops.insert(OpMath::Div.into(), env.functions.add(OpMath::Div, Box::new(math::div)));

        env.functions.ops = ops
    }

    fn env(&self) -> &EnvDb;
    fn env_mut(&mut self) -> &mut EnvDb;

    /// Add a `function` that is defined natively by [Code]
    fn add_lambda(&mut self, f: FunDef, body: Code) {
        if let Some(s) = self.env_mut().child.last_mut() {
            s.lambdas.add(f, body)
        } else {
            self.env_mut().lambdas.add(f, body)
        }
    }

    fn update_lambda(&mut self, f: FunDef, body: Code) {
        if let Some(s) = self.env_mut().child.last_mut() {
            s.lambdas.update(f, body)
        } else {
            self.env_mut().lambdas.update(f, body)
        }
    }

    /// Add a `ident` into the environment, similar to `let x = expr`
    fn add_ident(&mut self, name: &str, v: Code) {
        if let Some(s) = self.env_mut().child.last_mut() {
            s.idents.add(name, v)
        } else {
            self.env_mut().idents.add(name, v)
        }
    }

    /// Locates the `ident` in the environment
    fn find_ident(&self, key: &str) -> Option<&Code> {
        for s in self.env().child.iter().rev() {
            let ident = s.idents.get_by_name(key);
            if ident.is_some() {
                return ident;
            }
        }
        self.env().idents.get_by_name(key)
    }

    /// Allows to execute the query with the state carried by the implementation of this
    /// trait
    fn eval_query(&mut self, query: QueryCode) -> Result<Code, ErrorVm>;

    fn as_program_ref(&self) -> ProgramRef<'_>;
}

pub struct ProgramStore<P> {
    pub p: P,
    pub code: Code,
}

impl<P> ProgramStore<P> {
    pub fn new(p: P, code: Code) -> Self {
        Self { p, code }
    }
}

pub struct ProgramRef<'a> {
    pub env: &'a EnvDb,
    pub stats: &'a HashMap<String, u64>,
}

/// A default program that run in-memory without a database
pub struct Program {
    pub(crate) env: EnvDb,
    pub(crate) stats: HashMap<String, u64>,
}

impl Default for Program {
    fn default() -> Self {
        Self::new()
    }
}

impl Program {
    pub fn new() -> Self {
        let mut env = EnvDb::new();
        Self::load_ops(&mut env);
        Self {
            env,
            stats: Default::default(),
        }
    }
}

impl ProgramVm for Program {
    fn env(&self) -> &EnvDb {
        &self.env
    }

    fn env_mut(&mut self) -> &mut EnvDb {
        &mut self.env
    }

    fn eval_query(&mut self, query: QueryCode) -> Result<Code, ErrorVm> {
        let head = query.head();
        let row_count = query.row_count();
        let result = match query.data {
            Table::MemTable(x) => Box::new(RelIter::new(head, row_count, x)) as Box<IterRows<'_>>,
            Table::DbTable(_) => {
                panic!("DB not set")
            }
        };

        let result = build_query(result, query.query)?;

        let head = result.head().clone();
        let rows: Vec<_> = result.collect_vec()?;

        Ok(Code::Table(MemTable::new(&head, &rows)))
    }

    fn as_program_ref(&self) -> ProgramRef<'_> {
        ProgramRef {
            env: self.env(),
            stats: &self.stats,
        }
    }
}
