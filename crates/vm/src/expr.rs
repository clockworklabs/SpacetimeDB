use std::collections::HashMap;
use std::fmt;

use spacetimedb_sats::algebraic_type::AlgebraicType;
use spacetimedb_sats::algebraic_value::AlgebraicValue;
use spacetimedb_sats::relation::{
    DbTable, FieldExpr, FieldName, Header, MemTable, RelValueRef, Relation, RowCount, Table,
};
use spacetimedb_sats::satn::Satn;
use spacetimedb_sats::{TypeInSpace, Typespace};

use crate::errors::ErrorUser;
use crate::functions::{FunDef, Param};
use crate::operator::{Op, OpCmp, OpLogic, OpQuery};
use crate::types::Ty;

/// A `index` into the list of [Fun]
pub type FunctionId = usize;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TyExpr<T> {
    pub(crate) of: T,
    pub(crate) ty: Ty,
}

impl<T> TyExpr<T> {
    pub fn new(of: T, ty: Ty) -> Self {
        Self { of, ty }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Function {
    pub head: FunDef,
    pub body: Vec<Expr>,
}

impl Function {
    pub fn new(name: &str, params: &[Param], result: AlgebraicType, body: &[Expr]) -> Self {
        Self {
            head: FunDef::new(name, params, result),
            body: body.into(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FunctionOpt {
    pub(crate) head: FunDef,
    pub(crate) body: Vec<ExprOpt>,
}

impl FunctionOpt {
    pub fn new(head: FunDef, body: &[ExprOpt]) -> Self {
        Self {
            head,
            body: body.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ColumnOp {
    pub op: OpQuery,
    pub lhs: FieldExpr,
    pub rhs: FieldExpr,
}

impl ColumnOp {
    pub fn new(op: OpQuery, lhs: FieldExpr, rhs: FieldExpr) -> Self {
        Self { op, lhs, rhs }
    }

    pub fn compare(&self, row: RelValueRef) -> bool {
        let lhs = row.get(&self.lhs);
        let rhs = row.get(&self.rhs);
        dbg!(&lhs, self.op, &rhs);
        match self.op {
            OpQuery::Cmp(op) => match op {
                OpCmp::Eq => lhs == rhs,
                OpCmp::NotEq => lhs != rhs,
                OpCmp::Less => lhs < rhs,
                OpCmp::LessThan => lhs <= rhs,
                OpCmp::Greater => lhs > rhs,
                OpCmp::GreaterThan => lhs >= rhs,
            },
            OpQuery::Logic(op) => match op {
                OpLogic::And => match (lhs.as_bool(), rhs.as_bool()) {
                    (Some(lhs), Some(rhs)) => lhs && rhs,
                    _ => panic!("Operand 'and' not between boolean values"),
                },
                OpLogic::Or => match (lhs.as_bool(), rhs.as_bool()) {
                    (Some(lhs), Some(rhs)) => lhs || rhs,
                    _ => panic!("Operand 'or' not between boolean values"),
                },
            },
        }
    }
}

impl fmt::Display for ColumnOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.lhs, self.op, self.rhs)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct JoinExpr {
    pub(crate) rhs: SourceExpr,
    pub(crate) col_lhs: FieldName,
    pub(crate) col_rhs: FieldName,
}

impl JoinExpr {
    pub fn new(rhs: SourceExpr, col_lhs: FieldName, col_rhs: FieldName) -> Self {
        Self { rhs, col_lhs, col_rhs }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Query {
    Select(ColumnOp),
    Project(Vec<FieldName>),
    JoinInner(JoinExpr),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SourceExpr {
    Value(AlgebraicValue),
    MemTable(MemTable),
    DbTable(DbTable),
}

impl Relation for SourceExpr {
    fn head(&self) -> Header {
        match self {
            SourceExpr::Value(x) => Header::new(x.type_of().into()),
            SourceExpr::MemTable(x) => x.head(),
            SourceExpr::DbTable(x) => x.head(),
        }
    }

    fn row_count(&self) -> RowCount {
        match self {
            SourceExpr::Value(_) => RowCount::exact(1),
            SourceExpr::MemTable(x) => x.row_count(),
            SourceExpr::DbTable(x) => x.row_count(),
        }
    }
}

impl From<AlgebraicValue> for SourceExpr {
    fn from(x: AlgebraicValue) -> Self {
        Self::Value(x)
    }
}

impl From<MemTable> for SourceExpr {
    fn from(x: MemTable) -> Self {
        Self::MemTable(x)
    }
}

impl From<DbTable> for SourceExpr {
    fn from(x: DbTable) -> Self {
        Self::DbTable(x)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryExpr {
    pub source: SourceExpr,
    pub query: Vec<Query>,
}

impl QueryExpr {
    pub fn new<T: Into<SourceExpr>>(source: T) -> Self {
        Self {
            source: source.into(),
            query: vec![],
        }
    }

    pub fn with_select<LHS, RHS, O>(self, op: O, lhs: LHS, rhs: RHS) -> Self
    where
        LHS: Into<FieldExpr>,
        RHS: Into<FieldExpr>,
        O: Into<OpQuery>,
    {
        let op = ColumnOp::new(op.into(), lhs.into(), rhs.into());
        let mut x = self;
        x.query.push(Query::Select(op));
        x
    }

    pub fn with_project(self, cols: &[FieldName]) -> Self {
        let mut x = self;
        x.query.push(Query::Project(cols.into()));
        x
    }

    pub fn with_join_inner<Source>(self, with: Source, lhs: FieldName, rhs: FieldName) -> Self
    where
        Source: Into<SourceExpr>,
    {
        let mut x = self;
        x.query.push(Query::JoinInner(JoinExpr::new(with.into(), lhs, rhs)));
        x
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Expr {
    Value(AlgebraicValue),
    Ty(AlgebraicType),
    Op(Op, Vec<Expr>),
    Fun(Function),
    Block(Vec<Expr>),
    CallFn(String, HashMap<String, Expr>),
    Param(Box<(String, Expr)>),
    Let(Box<(String, Expr)>),
    Ident(String),
    If(Box<(Expr, Expr, Expr)>),
    Query(Box<QueryExpr>),
}

impl From<AlgebraicValue> for Expr {
    fn from(x: AlgebraicValue) -> Self {
        Expr::Value(x)
    }
}

impl From<QueryExpr> for Expr {
    fn from(x: QueryExpr) -> Self {
        Expr::Query(Box::new(x))
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SourceExprOpt {
    Value(TyExpr<AlgebraicValue>),
    MemTable(TyExpr<MemTable>),
    DbTable(TyExpr<DbTable>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryExprOpt {
    pub source: SourceExprOpt,
    pub(crate) query: Vec<Query>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ExprOpt {
    Value(TyExpr<AlgebraicValue>),
    Ty(Ty),
    Op(TyExpr<Op>, Vec<ExprOpt>),
    Fun(FunctionOpt),
    CallFn(String, Vec<ExprOpt>),
    CallLambda(String, HashMap<String, ExprOpt>),
    Param(Box<(String, ExprOpt)>),
    Let(Box<(String, ExprOpt)>),
    Ident(String),
    If(Box<(ExprOpt, ExprOpt, ExprOpt)>),
    Block(Vec<ExprOpt>),
    Query(Box<QueryExprOpt>),
    Halt(ErrorUser),
}

pub(crate) fn fmt_value(ty: &AlgebraicType, val: &AlgebraicValue) -> String {
    let ts = Typespace::new(vec![]);
    TypeInSpace::new(&ts, ty).with_value(val).to_satn()
}

impl fmt::Display for SourceExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceExpr::Value(x) => {
                let ty = x.type_of();
                let x = fmt_value(&ty, x);
                write!(f, "{x}")
            }
            SourceExpr::MemTable(x) => {
                let ty = &AlgebraicType::Product(x.head().head);
                for row in &x.data {
                    let val = AlgebraicValue::Product(row.clone());
                    let x = fmt_value(ty, &val);
                    write!(f, "{x}")?;
                }
                Ok(())
            }
            SourceExpr::DbTable(x) => {
                write!(f, "DbTable({})", x.table_id)
            }
        }
    }
}

impl fmt::Display for SourceExprOpt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceExprOpt::Value(x) => {
                let ty = match &x.ty {
                    Ty::Val(x) => x,
                    x => unreachable!("{}", x),
                };
                let x = fmt_value(ty, &x.of);
                write!(f, "{x}")
            }
            SourceExprOpt::MemTable(x) => {
                write!(f, "{:?}", x.of)
            }
            SourceExprOpt::DbTable(x) => {
                write!(f, "{:?}", x.of)
            }
        }
    }
}

impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Query::Select(q) => {
                write!(f, "select {q}")
            }
            Query::Project(q) => {
                write!(f, "project")?;
                if !q.is_empty() {
                    write!(f, " ")?;
                }
                for (pos, x) in q.iter().enumerate() {
                    write!(f, "{x}")?;
                    if pos + 1 < q.len() {
                        write!(f, ", ")?;
                    }
                }
                Ok(())
            }
            Query::JoinInner(q) => {
                write!(f, "&inner {} ON {} = {}", q.rhs, q.col_lhs, q.col_rhs)
            }
        }
    }
}

impl fmt::Display for ExprOpt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExprOpt::Value(x) => {
                write!(f, "{:?}", &x.of)
            }
            ExprOpt::Ty(x) => {
                write!(f, "{:?}", &x)
            }

            ExprOpt::Op(op, _) => {
                write!(f, "{:?}", op.of)
            }
            ExprOpt::Fun(x) => {
                write!(f, "fn {}({:?}):{:?}", x.head.name, x.head.params, x.head.result)?;
                writeln!(f, "{{")?;
                writeln!(f, "{:?}", x.body)?;
                writeln!(f, "}}")
            }
            ExprOpt::CallFn(x, params) => {
                write!(f, "{}(", x)?;
                for (pos, v) in params.iter().enumerate() {
                    write!(f, "{v}")?;
                    if pos + 1 < params.len() {
                        write!(f, ", ")?;
                    }
                }
                write!(f, ")",)
            }
            ExprOpt::CallLambda(x, params) => {
                write!(f, "{}(", x)?;
                for (pos, (k, v)) in params.iter().enumerate() {
                    write!(f, "{k} = {v}")?;
                    if pos + 1 < params.len() {
                        write!(f, ", ")?;
                    }
                }
                write!(f, ")",)
            }
            ExprOpt::Param(inner) => {
                let (name, p) = &**inner;
                write!(f, "{name} = {p}")
            }
            ExprOpt::Let(x) => {
                write!(f, "{:?}", x)
            }
            ExprOpt::Ident(x) => {
                write!(f, "{}", x)
            }
            ExprOpt::If(inner) => {
                let (test, if_true, if_false) = &**inner;
                write!(f, "if {test}\n\t{if_true}else\n\t{if_false}else")
            }
            ExprOpt::Halt(x) => {
                write!(f, "{}", x)
            }
            ExprOpt::Query(q) => {
                write!(f, "{}", q.source)?;
                for op in &q.query {
                    write!(f, "?{op}")?;
                }
                Ok(())
            }
            ExprOpt::Block(lines) => {
                for x in lines {
                    writeln!(f, "{x}")?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryCode {
    pub data: Table,
    pub query: Vec<Query>,
}

impl Relation for QueryCode {
    fn head(&self) -> Header {
        self.data.head()
    }

    fn row_count(&self) -> RowCount {
        self.data.row_count()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Code {
    Value(AlgebraicValue),
    Table(MemTable),
    CallFn(FunctionId, Vec<Code>),
    CallLambda(FunctionId, HashMap<String, Code>),
    If(Box<(Code, Code, Code)>),
    Ident(String),
    Halt(ErrorUser),
    Fun(FunctionId),
    Block(Vec<Code>),
    Query(QueryCode),
    Pass,
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Code::Value(x) => {
                write!(f, "{:?}", &x)
            }
            Code::CallFn(name, _) => {
                write!(f, "Fn({})", name)
            }
            Code::Block(_) => write!(f, "Block"),
            Code::If(_) => write!(f, "If"),
            x => todo!("{:?}", x),
        }
    }
}
