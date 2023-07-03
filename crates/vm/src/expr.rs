use spacetimedb_lib::auth::{StAccess, StTableType};
use spacetimedb_lib::error::AuthError;
use spacetimedb_lib::table::ProductTypeMeta;
use spacetimedb_lib::Identity;
use std::collections::HashMap;
use std::fmt;

use spacetimedb_lib::relation::{
    DbTable, FieldExpr, FieldName, Header, MemTable, RelValueRef, Relation, RowCount, Table,
};
use spacetimedb_sats::algebraic_type::AlgebraicType;
use spacetimedb_sats::algebraic_value::AlgebraicValue;
use spacetimedb_sats::satn::Satn;
use spacetimedb_sats::{ProductValue, Typespace, WithTypespace};

use crate::errors::{ErrorKind, ErrorLang, ErrorType, ErrorVm};
use crate::functions::{FunDef, Param};
use crate::operator::{Op, OpCmp, OpLogic, OpQuery};
use crate::types::Ty;

/// A `index` into the list of [Fun]
pub type FunctionId = usize;

/// Trait for checking if the `caller` have access to `Self`
pub trait AuthAccess {
    fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError>;
}

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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ColumnOp {
    Field(FieldExpr),
    Cmp {
        op: OpQuery,
        lhs: Box<ColumnOp>,
        rhs: Box<ColumnOp>,
    },
}

impl ColumnOp {
    pub fn cmp(op: OpQuery, lhs: ColumnOp, rhs: ColumnOp) -> Self {
        Self::Cmp {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }

    fn reduce(&self, row: RelValueRef, value: &ColumnOp) -> Result<AlgebraicValue, ErrorLang> {
        match value {
            ColumnOp::Field(field) => Ok(row.get(field).clone()),
            ColumnOp::Cmp { op, lhs, rhs } => Ok(self.compare_bin_op(row, *op, lhs, rhs)?.into()),
        }
    }

    fn reduce_bool(&self, row: RelValueRef, value: &ColumnOp) -> Result<bool, ErrorLang> {
        match value {
            ColumnOp::Field(field) => {
                let field = row.get(field);

                match field.as_bool() {
                    Some(b) => Ok(*b),
                    None => Err(ErrorType::FieldBool(field.clone()).into()),
                }
            }
            ColumnOp::Cmp { op, lhs, rhs } => Ok(self.compare_bin_op(row, *op, lhs, rhs)?),
        }
    }

    fn compare_bin_op(&self, row: RelValueRef, op: OpQuery, lhs: &ColumnOp, rhs: &ColumnOp) -> Result<bool, ErrorVm> {
        match op {
            OpQuery::Cmp(op) => {
                let lhs = self.reduce(row, lhs)?;
                let rhs = self.reduce(row, rhs)?;

                Ok(match op {
                    OpCmp::Eq => lhs == rhs,
                    OpCmp::NotEq => lhs != rhs,
                    OpCmp::Lt => lhs < rhs,
                    OpCmp::LtEq => lhs <= rhs,
                    OpCmp::Gt => lhs > rhs,
                    OpCmp::GtEq => lhs >= rhs,
                })
            }
            OpQuery::Logic(op) => {
                let lhs = self.reduce_bool(row, lhs)?;
                let rhs = self.reduce_bool(row, rhs)?;

                Ok(match op {
                    OpLogic::And => lhs && rhs,
                    OpLogic::Or => lhs || rhs,
                })
            }
        }
    }

    pub fn compare(&self, row: RelValueRef) -> Result<bool, ErrorVm> {
        match self {
            ColumnOp::Field(field) => {
                let lhs = row.get(field);
                Ok(*lhs.as_bool().unwrap())
            }
            ColumnOp::Cmp { op, lhs, rhs } => self.compare_bin_op(row, *op, lhs, rhs),
        }
    }
}

impl fmt::Display for ColumnOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ColumnOp::Field(x) => {
                write!(f, "{}", x)
            }
            ColumnOp::Cmp { op, lhs, rhs } => {
                write!(f, "{} {} {}", lhs, op, rhs)
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub enum SourceExpr {
    MemTable(MemTable),
    DbTable(DbTable),
}

impl SourceExpr {
    pub fn get_db_table(&self) -> Option<&DbTable> {
        match self {
            SourceExpr::DbTable(x) => Some(x),
            _ => None,
        }
    }

    pub fn table_name(&self) -> &str {
        match self {
            SourceExpr::MemTable(x) => &x.head.table_name,
            SourceExpr::DbTable(x) => &x.head.table_name,
        }
    }

    pub fn table_type(&self) -> StTableType {
        match self {
            SourceExpr::MemTable(_) => StTableType::User,
            SourceExpr::DbTable(x) => x.table_type,
        }
    }

    pub fn table_access(&self) -> StAccess {
        match self {
            SourceExpr::MemTable(x) => x.table_access,
            SourceExpr::DbTable(x) => x.table_access,
        }
    }
}

impl Relation for SourceExpr {
    fn head(&self) -> Header {
        match self {
            SourceExpr::MemTable(x) => x.head(),
            SourceExpr::DbTable(x) => x.head(),
        }
    }

    fn row_count(&self) -> RowCount {
        match self {
            SourceExpr::MemTable(x) => x.row_count(),
            SourceExpr::DbTable(x) => x.row_count(),
        }
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

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct JoinExpr {
    pub rhs: SourceExpr,
    pub col_lhs: FieldName,
    pub col_rhs: FieldName,
}

impl JoinExpr {
    pub fn new(rhs: SourceExpr, col_lhs: FieldName, col_rhs: FieldName) -> Self {
        Self { rhs, col_lhs, col_rhs }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
pub enum DbType {
    Table,
    Index,
    Sequence,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
pub enum Crud {
    Query,
    Insert,
    Update,
    Delete,
    Create(DbType),
    Drop(DbType),
}

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub enum CrudExpr {
    Query(QueryExpr),
    Insert {
        source: SourceExpr,
        rows: Vec<Vec<FieldExpr>>,
    },
    Update {
        insert: QueryExpr,
        delete: QueryExpr,
    },
    Delete {
        query: QueryExpr,
    },
    CreateTable {
        name: String,
        columns: ProductTypeMeta,
        table_type: StTableType,
        table_access: StAccess,
    },
    Drop {
        name: String,
        kind: DbType,
        table_access: StAccess,
    },
}

// impl AuthAccess for CrudExpr {
//     fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError> {
//         if owner == caller {
//             return Ok(());
//         };
//         match self {
//             CrudExpr::Query(from) => {
//                 from.source.table_access() == StAccess::Public && from.query.iter().any(|x| x.check_auth(owner, caller))
//             }
//             CrudExpr::Insert { source, .. } => source.table_access() == StAccess::Public,
//             CrudExpr::Update { insert, delete } => insert.check_auth(owner, caller) && delete.check_auth(owner, caller),
//             CrudExpr::Delete { query, .. } => query.check_auth(owner, caller),
//             CrudExpr::CreateTable { table_access, .. } => table_access == &StAccess::Public,
//             CrudExpr::Drop { .. } => Ok(()),
//         }
//     }
// }

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub enum Query {
    Select(ColumnOp),
    Project(Vec<FieldExpr>),
    JoinInner(JoinExpr),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
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

    pub fn with_select<O>(self, op: O) -> Self
    where
        O: Into<ColumnOp>,
    {
        let mut x = self;
        x.query.push(Query::Select(op.into()));
        x
    }

    pub fn with_select_cmp<LHS, RHS, O>(self, op: O, lhs: LHS, rhs: RHS) -> Self
    where
        LHS: Into<FieldExpr>,
        RHS: Into<FieldExpr>,
        O: Into<OpQuery>,
    {
        let op = ColumnOp::cmp(op.into(), ColumnOp::Field(lhs.into()), ColumnOp::Field(rhs.into()));
        self.with_select(op)
    }

    pub fn with_project(self, cols: &[FieldExpr]) -> Self {
        let mut x = self;
        if !cols.is_empty() {
            x.query.push(Query::Project(cols.into()));
        }
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

impl AuthAccess for Query {
    fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError> {
        if owner == caller {
            Ok(())
        } else if let Query::JoinInner(j) = self {
            if j.rhs.table_access() == StAccess::Public {
                Ok(())
            } else {
                Err(AuthError::TablePrivate {
                    named: j.rhs.table_name().to_string(),
                })
            }
        } else {
            Ok(())
        }
    }
}
//
// impl AuthAccess for QueryExpr {
//     fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError> {
//         self.source.table_access() == StAccess::Public && self.query.iter().any(|x| x.check_auth(owner, caller))
//     }
// }

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
    Crud(Box<CrudExpr>),
}

impl From<AlgebraicValue> for Expr {
    fn from(x: AlgebraicValue) -> Self {
        Expr::Value(x)
    }
}

impl From<QueryExpr> for Expr {
    fn from(x: QueryExpr) -> Self {
        Expr::Crud(Box::new(CrudExpr::Query(x)))
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
pub enum CrudExprOpt {
    Insert {
        source: SourceExprOpt,
        rows: Vec<ProductValue>,
    },
    Update {
        insert: QueryExprOpt,
        delete: QueryExprOpt,
    },
    Delete {
        query: QueryExprOpt,
    },
    CreateTable {
        name: String,
        columns: ProductTypeMeta,
        table_type: StTableType,
        table_access: StAccess,
    },
    Drop {
        name: String,
        kind: DbType,
        table_access: StAccess,
    },
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
    Crud(Box<CrudExprOpt>),
    Halt(ErrorLang),
}

pub(crate) fn fmt_value(ty: &AlgebraicType, val: &AlgebraicValue) -> String {
    let ts = Typespace::new(vec![]);
    WithTypespace::new(&ts, ty).with_value(val).to_satn()
}

impl fmt::Display for SourceExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceExpr::MemTable(x) => {
                let ty = &AlgebraicType::Product(x.head().ty());
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
                    x => unreachable!("Formatting of `{}`", x),
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
            ExprOpt::Crud(x) => {
                let x = &**x;
                match x {
                    CrudExprOpt::Insert { source, rows } => {
                        write!(f, "{}", source)?;
                        for row in rows {
                            write!(f, "{row:?}")?;
                        }
                    }
                    CrudExprOpt::Update { .. } => {}
                    CrudExprOpt::Delete { .. } => {}
                    CrudExprOpt::CreateTable { .. } => {}
                    CrudExprOpt::Drop { .. } => {}
                };
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
    pub table: Table,
    pub query: Vec<Query>,
}

impl AuthAccess for Table {
    fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError> {
        if owner == caller || self.table_access() == StAccess::Public {
            return Ok(());
        }

        Err(AuthError::TablePrivate {
            named: self.table_name().to_string(),
        })
    }
}

impl AuthAccess for QueryCode {
    fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError> {
        if owner == caller {
            return Ok(());
        }
        self.table.check_auth(owner, caller)?;

        if let Some(err) = self.query.iter().find_map(|x| {
            if let Err(err) = x.check_auth(owner, caller) {
                Some(err)
            } else {
                None
            }
        }) {
            Err(err)
        } else {
            Ok(())
        }
    }
}

impl Relation for QueryCode {
    fn head(&self) -> Header {
        self.table.head()
    }

    fn row_count(&self) -> RowCount {
        self.table.row_count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrudCode {
    Query(QueryCode),
    Insert {
        table: Table,
        rows: Vec<ProductValue>,
    },
    Update {
        insert: QueryCode,
        delete: QueryCode,
    },
    Delete {
        query: QueryCode,
    },
    CreateTable {
        name: String,
        columns: ProductTypeMeta,
        table_type: StTableType,
        table_access: StAccess,
    },
    Drop {
        name: String,
        kind: DbType,
        table_access: StAccess,
    },
}

impl AuthAccess for CrudCode {
    fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError> {
        if owner == caller {
            return Ok(());
        }
        match self {
            CrudCode::Query(q) => q.check_auth(owner, caller),
            CrudCode::Insert { table, .. } => table.check_auth(owner, caller),
            CrudCode::Update { insert, delete } => {
                insert.check_auth(owner, caller)?;
                delete.check_auth(owner, caller)
            }
            CrudCode::Delete { query, .. } => query.check_auth(owner, caller),
            //TODO: Must allow to create private tables for `caller`
            CrudCode::CreateTable { name, table_access, .. } => {
                if table_access == &StAccess::Public {
                    Ok(())
                } else {
                    Err(AuthError::TablePrivate {
                        named: name.to_string(),
                    })
                }
            }
            CrudCode::Drop {
                name,
                kind,
                table_access,
            } => {
                if table_access == &StAccess::Public {
                    Ok(())
                } else {
                    let named = name.to_string();
                    Err(match kind {
                        DbType::Table => AuthError::TablePrivate { named },
                        DbType::Index => AuthError::IndexPrivate { named },
                        DbType::Sequence => AuthError::SequencePrivate { named },
                    })
                }
            }
        }
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
    Halt(ErrorLang),
    Fun(FunctionId),
    Block(Vec<Code>),
    Crud(CrudCode),
    Pass,
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Code::Value(x) => {
                write!(f, "{:?}", &x)
            }
            Code::CallFn(id, _) => {
                write!(f, "Fn({})", id)
            }
            Code::Block(_) => write!(f, "Block"),
            Code::If(_) => write!(f, "If"),
            x => todo!("{:?}", x),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CodeResult {
    Value(AlgebraicValue),
    Table(MemTable),
    Block(Vec<CodeResult>),
    Halt(ErrorLang),
    Pass,
}

impl From<Code> for CodeResult {
    fn from(code: Code) -> Self {
        match code {
            Code::Value(x) => Self::Value(x),
            Code::Table(x) => Self::Table(x),
            Code::Halt(x) => Self::Halt(x),
            Code::Block(x) => {
                if x.is_empty() {
                    Self::Pass
                } else {
                    Self::Block(x.into_iter().map(CodeResult::from).collect())
                }
            }
            Code::Pass => Self::Pass,
            x => Self::Halt(ErrorLang::new(
                ErrorKind::Compiler,
                Some(&format!("Invalid result: {x}")),
            )),
        }
    }
}
