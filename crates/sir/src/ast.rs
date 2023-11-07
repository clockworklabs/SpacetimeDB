use crate::errors::{ErrorLang, ErrorVm};
use derive_more::From;
use nonempty::NonEmpty;
use spacetimedb_lib::operator::OpLogic;
use spacetimedb_lib::relation::MemTable;
use spacetimedb_primitives::{ColId, IndexId, TableId};
use spacetimedb_sats::AlgebraicValue;

pub type IdentId = u32; //an index into the environment

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Block {
    sir: Vec<Sir>,
}

// We turn logical expressions (for WHERE, IF, WHILE, ...) into this closure for execution
type FnBool = dyn Fn() -> bool;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CmpExpr {
    Col(ColId),
    Val(AlgebraicValue),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Cmp {
    op: OpLogic,
    lhs: CmpExpr,
    rhs: CmpExpr,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum BoolExpr {
    Constant(bool),
    //Expr(Box<FnBool>),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RelId {
    DbTable(TableId),
    MemTable(TableId),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum QirOp {
    Scan,
    Project(NonEmpty<ColId>),
    ColSeek(Cmp),
    IndexSeek(IndexId, Cmp),
    Join(Box<Qir>),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Qir {
    pub(crate) source: RelId,
    pub(crate) ops: NonEmpty<QirOp>,
}

impl Qir {
    pub fn new(source: RelId, root: QirOp) -> Self {
        Self {
            source,
            ops: NonEmpty::new(root),
        }
    }

    pub fn with(self, op: QirOp) -> Self {
        let mut x = self;
        x.ops.push(op);
        x
    }
}

#[derive(Debug, Clone, Eq, PartialEq, From)]
pub enum Sir {
    #[from]
    Value(AlgebraicValue),

    //Declarations
    /// let a = 1 // immutable
    Let(IdentId, Box<Sir>),
    /// var a = 1 // mutable
    Var(IdentId, Box<Sir>),
    /// a = 2 -- changing a var
    Set(IdentId, Box<Sir>),

    /// if a = 1 { b } else { c }
    If(BoolExpr, Block, Block),
    /// while true {...}
    While(BoolExpr, Block),

    /// DB Calls
    #[from]
    Qir(Qir),

    /// Ignore or "void", like if a = 1 { print("yes") } else { pass }
    Pass,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SirResult {
    Value(AlgebraicValue),
    Table(MemTable),
    Block(Vec<SirResult>),
    Halt(ErrorLang),
    Pass,
}

impl From<Result<SirResult, ErrorVm>> for SirResult {
    fn from(x: Result<SirResult, ErrorVm>) -> Self {
        match x {
            Ok(x) => x,
            Err(err) => SirResult::Halt(err.into()),
        }
    }
}
