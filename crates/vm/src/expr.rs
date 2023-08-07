use derive_more::From;
use spacetimedb_lib::auth::{StAccess, StTableType};
use spacetimedb_lib::error::AuthError;
use spacetimedb_lib::relation::{
    DbTable, FieldExpr, FieldName, Header, MemTable, RelValueRef, Relation, RowCount, Table,
};
use spacetimedb_lib::table::ProductTypeMeta;
use spacetimedb_lib::Identity;
use spacetimedb_sats::algebraic_type::AlgebraicType;
use spacetimedb_sats::algebraic_value::AlgebraicValue;
use spacetimedb_sats::satn::Satn;
use spacetimedb_sats::{ProductValue, SatsString, Typespace, WithTypespace};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::ops::Bound;

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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, From)]
pub enum ColumnOp {
    #[from]
    Field(FieldExpr),
    Cmp {
        op: OpQuery,
        lhs: Box<ColumnOp>,
        rhs: Box<ColumnOp>,
    },
}

impl ColumnOp {
    pub fn new(op: OpQuery, lhs: ColumnOp, rhs: ColumnOp) -> Self {
        Self::Cmp {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }

    pub fn cmp(field: FieldName, op: OpCmp, value: AlgebraicValue) -> Self {
        Self::Cmp {
            op: OpQuery::Cmp(op),
            lhs: Box::new(ColumnOp::Field(FieldExpr::Name(field))),
            rhs: Box::new(ColumnOp::Field(FieldExpr::Value(value))),
        }
    }

    fn reduce(&self, row: RelValueRef, value: &ColumnOp, header: &Header) -> Result<AlgebraicValue, ErrorLang> {
        match value {
            ColumnOp::Field(field) => Ok(row.get(field, header).clone()),
            ColumnOp::Cmp { op, lhs, rhs } => Ok(self.compare_bin_op(row, *op, lhs, rhs, header)?.into()),
        }
    }

    fn reduce_bool(&self, row: RelValueRef, value: &ColumnOp, header: &Header) -> Result<bool, ErrorLang> {
        match value {
            ColumnOp::Field(field) => {
                let field = row.get(field, header);

                match field.as_bool() {
                    Some(b) => Ok(*b),
                    None => Err(ErrorType::FieldBool(field.clone()).into()),
                }
            }
            ColumnOp::Cmp { op, lhs, rhs } => Ok(self.compare_bin_op(row, *op, lhs, rhs, header)?),
        }
    }

    fn compare_bin_op(
        &self,
        row: RelValueRef,
        op: OpQuery,
        lhs: &ColumnOp,
        rhs: &ColumnOp,
        header: &Header,
    ) -> Result<bool, ErrorVm> {
        match op {
            OpQuery::Cmp(op) => {
                let lhs = self.reduce(row, lhs, header)?;
                let rhs = self.reduce(row, rhs, header)?;

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
                let lhs = self.reduce_bool(row, lhs, header)?;
                let rhs = self.reduce_bool(row, rhs, header)?;

                Ok(match op {
                    OpLogic::And => lhs && rhs,
                    OpLogic::Or => lhs || rhs,
                })
            }
        }
    }

    pub fn compare(&self, row: RelValueRef, header: &Header) -> Result<bool, ErrorVm> {
        match self {
            ColumnOp::Field(field) => {
                let lhs = row.get(field, header);
                Ok(*lhs.as_bool().unwrap())
            }
            ColumnOp::Cmp { op, lhs, rhs } => self.compare_bin_op(row, *op, lhs, rhs, header),
        }
    }

    // Flattens a nested conjunction of AND expressions.
    pub fn to_vec(self) -> Vec<ColumnOp> {
        match self {
            ColumnOp::Cmp {
                op: OpQuery::Logic(OpLogic::And),
                lhs,
                rhs,
            } => {
                let mut lhs = lhs.to_vec();
                let mut rhs = rhs.to_vec();
                lhs.append(&mut rhs);
                lhs
            }
            op => vec![op],
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

impl From<FieldName> for ColumnOp {
    fn from(value: FieldName) -> Self {
        ColumnOp::Field(value.into())
    }
}

impl From<FieldName> for Box<ColumnOp> {
    fn from(value: FieldName) -> Self {
        Box::new(ColumnOp::Field(value.into()))
    }
}

impl From<AlgebraicValue> for ColumnOp {
    fn from(value: AlgebraicValue) -> Self {
        ColumnOp::Field(value.into())
    }
}

impl From<AlgebraicValue> for Box<ColumnOp> {
    fn from(value: AlgebraicValue) -> Self {
        Box::new(ColumnOp::Field(value.into()))
    }
}

impl From<IndexScan> for Box<ColumnOp> {
    fn from(value: IndexScan) -> Self {
        Box::new(value.into())
    }
}

impl From<IndexScan> for ColumnOp {
    fn from(value: IndexScan) -> Self {
        let table = value.table;
        let col_id = value.col_id;
        let field = table.head.fields[col_id as usize].field.clone();
        match (value.lower_bound, value.upper_bound) {
            // Inclusive lower bound => field >= value
            (Bound::Included(value), Bound::Unbounded) => ColumnOp::Cmp {
                op: OpQuery::Cmp(OpCmp::GtEq),
                lhs: field.into(),
                rhs: value.into(),
            },
            // Exclusive lower bound => field > value
            (Bound::Excluded(value), Bound::Unbounded) => ColumnOp::Cmp {
                op: OpQuery::Cmp(OpCmp::Gt),
                lhs: field.into(),
                rhs: value.into(),
            },
            // Inclusive upper bound => field <= value
            (Bound::Unbounded, Bound::Included(value)) => ColumnOp::Cmp {
                op: OpQuery::Cmp(OpCmp::LtEq),
                lhs: field.into(),
                rhs: value.into(),
            },
            // Exclusive upper bound => field < value
            (Bound::Unbounded, Bound::Excluded(value)) => ColumnOp::Cmp {
                op: OpQuery::Cmp(OpCmp::Lt),
                lhs: field.into(),
                rhs: value.into(),
            },
            (Bound::Unbounded, Bound::Unbounded) => unreachable!(),
            (lower_bound, upper_bound) => {
                let lhs = IndexScan {
                    table: table.clone(),
                    col_id,
                    lower_bound,
                    upper_bound: Bound::Unbounded,
                };
                let rhs = IndexScan {
                    table,
                    col_id,
                    lower_bound: Bound::Unbounded,
                    upper_bound,
                };
                ColumnOp::Cmp {
                    op: OpQuery::Logic(OpLogic::And),
                    lhs: lhs.into(),
                    rhs: rhs.into(),
                }
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, From)]
pub enum SourceExpr {
    MemTable(MemTable),
    DbTable(DbTable),
}

impl From<Table> for SourceExpr {
    fn from(value: Table) -> Self {
        match value {
            Table::MemTable(t) => SourceExpr::MemTable(t),
            Table::DbTable(t) => SourceExpr::DbTable(t),
        }
    }
}

impl From<SourceExpr> for Table {
    fn from(value: SourceExpr) -> Self {
        match value {
            SourceExpr::MemTable(t) => Table::MemTable(t),
            SourceExpr::DbTable(t) => Table::DbTable(t),
        }
    }
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

// A descriptor for an index join operation.
// The semantics are that of a semi-join with rows from the index side being returned.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct IndexJoin {
    pub probe_side: QueryExpr,
    pub probe_field: FieldName,
    pub index_header: Header,
    pub index_table: u32,
    pub index_col: u32,
}

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct JoinExpr {
    pub rhs: QueryExpr,
    pub col_lhs: FieldName,
    pub col_rhs: FieldName,
}

impl From<IndexJoin> for JoinExpr {
    fn from(value: IndexJoin) -> Self {
        let pos = value.index_col as usize;
        let rhs = value.probe_side;
        let col_lhs = value.index_header.fields[pos].field.clone();
        let col_rhs = value.probe_field;
        JoinExpr::new(rhs, col_lhs, col_rhs)
    }
}

impl JoinExpr {
    pub fn new(rhs: QueryExpr, col_lhs: FieldName, col_rhs: FieldName) -> Self {
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
        name: SatsString,
        columns: ProductTypeMeta,
        table_type: StTableType,
        table_access: StAccess,
    },
    Drop {
        name: SatsString,
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct IndexScan {
    pub table: DbTable,
    pub col_id: u32,
    pub lower_bound: Bound<AlgebraicValue>,
    pub upper_bound: Bound<AlgebraicValue>,
}

impl PartialOrd for IndexScan {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IndexScan {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        #[derive(Eq, PartialEq)]
        struct RangeBound<'a, T: Ord>(&'a Bound<T>);

        impl<'a, T: Ord> PartialOrd for RangeBound<'a, T> {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        impl<'a, T: Ord> Ord for RangeBound<'a, T> {
            fn cmp(&self, other: &Self) -> Ordering {
                match (&self.0, &other.0) {
                    (Bound::Included(ref l), Bound::Included(ref r))
                    | (Bound::Excluded(ref l), Bound::Excluded(ref r)) => l.cmp(r),
                    (Bound::Included(ref l), Bound::Excluded(ref r)) => match l.cmp(r) {
                        Ordering::Equal => Ordering::Less,
                        ord => ord,
                    },
                    (Bound::Excluded(ref l), Bound::Included(ref r)) => match l.cmp(r) {
                        Ordering::Equal => Ordering::Greater,
                        ord => ord,
                    },
                    (Bound::Unbounded, Bound::Unbounded) => Ordering::Equal,
                    (Bound::Unbounded, _) => Ordering::Less,
                    (_, Bound::Unbounded) => Ordering::Greater,
                }
            }
        }

        let order = self.table.cmp(&other.table);
        let Ordering::Equal = order else {
            return order;
        };

        let order = self.col_id.cmp(&other.col_id);
        let Ordering::Equal = order else {
            return order;
        };

        match (
            RangeBound(&self.lower_bound).cmp(&RangeBound(&other.lower_bound)),
            RangeBound(&self.upper_bound).cmp(&RangeBound(&other.upper_bound)),
        ) {
            (Ordering::Equal, ord) => ord,
            (ord, _) => ord,
        }
    }
}

// An individual operation in a query.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub enum Query {
    // Fetching rows via an index.
    IndexScan(IndexScan),
    // Joining rows via an index.
    // Equivalent to Index Nested Loop Join.
    IndexJoin(IndexJoin),
    // A filter over an intermediate relation.
    // In particular it does not utilize any indexes.
    // If it could it would have already been transformed into an IndexScan.
    Select(ColumnOp),
    // Projects a set of columns.
    // The second argument is the table id for a qualified wildcard project.
    // If present, further optimzations are possible.
    Project(Vec<FieldExpr>, Option<u32>),
    // A join of two relations (base or intermediate) based on equality.
    // Equivalent to a Nested Loop Join.
    // Its operands my use indexes but the join itself does not.
    JoinInner(JoinExpr),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct QueryExpr {
    pub source: SourceExpr,
    pub query: Vec<Query>,
}

impl From<MemTable> for QueryExpr {
    fn from(value: MemTable) -> Self {
        QueryExpr {
            source: value.into(),
            query: vec![],
        }
    }
}

impl From<DbTable> for QueryExpr {
    fn from(value: DbTable) -> Self {
        QueryExpr {
            source: value.into(),
            query: vec![],
        }
    }
}

impl QueryExpr {
    pub fn new<T: Into<SourceExpr>>(source: T) -> Self {
        Self {
            source: source.into(),
            query: vec![],
        }
    }

    // Generate an index scan for an equality predicate if this is the first operator.
    // Otherwise generate a select.
    // TODO: Replace these methods with a proper query optimization pass.
    pub fn with_index_eq(mut self, table: DbTable, col_id: u32, value: AlgebraicValue) -> Self {
        // if this is the first operator in the list, generate index scan
        let Some(query) = self.query.pop() else {
            self.query.push(Query::IndexScan(IndexScan {
                table,
                col_id,
                lower_bound: Bound::Included(value.clone()),
                upper_bound: Bound::Included(value),
            }));
            return self;
        };
        match query {
            // try to push below join's lhs
            Query::JoinInner(JoinExpr {
                rhs:
                    QueryExpr {
                        source:
                            SourceExpr::DbTable(DbTable {
                                table_id: rhs_table_id, ..
                            }),
                        ..
                    },
                ..
            }) if table.table_id != rhs_table_id => {
                self = self.with_index_eq(table, col_id, value);
                self.query.push(query);
                self
            }
            // try to push below join's rhs
            Query::JoinInner(JoinExpr { rhs, col_lhs, col_rhs }) => {
                self.query.push(Query::JoinInner(JoinExpr {
                    rhs: rhs.with_index_eq(table, col_id, value),
                    col_lhs,
                    col_rhs,
                }));
                self
            }
            // merge with a preceding select
            Query::Select(filter) => {
                self.query.push(Query::Select(ColumnOp::Cmp {
                    op: OpQuery::Logic(OpLogic::And),
                    lhs: filter.into(),
                    rhs: IndexScan {
                        table,
                        col_id,
                        lower_bound: Bound::Included(value.clone()),
                        upper_bound: Bound::Included(value),
                    }
                    .into(),
                }));
                self
            }
            // else generate a new select
            query => {
                self.query.push(query);
                self.query.push(Query::Select(
                    IndexScan {
                        table,
                        col_id,
                        lower_bound: Bound::Included(value.clone()),
                        upper_bound: Bound::Included(value),
                    }
                    .into(),
                ));
                self
            }
        }
    }

    // Generate an index scan for a range predicate or try merging with a previous index scan.
    // Otherwise generate a select.
    // TODO: Replace these methods with a proper query optimization pass.
    pub fn with_index_lower_bound(
        mut self,
        table: DbTable,
        col_id: u32,
        value: AlgebraicValue,
        inclusive: bool,
    ) -> Self {
        // if this is the first operator in the list, generate an index scan
        let Some(query) = self.query.pop() else {
            self.query.push(Query::IndexScan(IndexScan {
                table,
                col_id,
                lower_bound: Self::bound(value, inclusive),
                upper_bound: Bound::Unbounded,
            }));
            return self;
        };
        match query {
            // try to push below join's lhs
            Query::JoinInner(JoinExpr {
                rhs:
                    QueryExpr {
                        source:
                            SourceExpr::DbTable(DbTable {
                                table_id: rhs_table_id, ..
                            }),
                        ..
                    },
                ..
            }) if table.table_id != rhs_table_id => {
                self = self.with_index_lower_bound(table, col_id, value, inclusive);
                self.query.push(query);
                self
            }
            // try to push below join's rhs
            Query::JoinInner(JoinExpr { rhs, col_lhs, col_rhs }) => {
                self.query.push(Query::JoinInner(JoinExpr {
                    rhs: rhs.with_index_lower_bound(table, col_id, value, inclusive),
                    col_lhs,
                    col_rhs,
                }));
                self
            }
            // merge with a preceding upper bounded index scan (inclusive)
            Query::IndexScan(IndexScan {
                col_id: lhs_col_id,
                lower_bound: Bound::Unbounded,
                upper_bound: Bound::Included(upper),
                ..
            }) if col_id == lhs_col_id => {
                self.query.push(Query::IndexScan(IndexScan {
                    table,
                    col_id,
                    lower_bound: Self::bound(value, inclusive),
                    upper_bound: Bound::Included(upper),
                }));
                self
            }
            // merge with a preceding upper bounded index scan (exclusive)
            Query::IndexScan(IndexScan {
                col_id: lhs_col_id,
                lower_bound: Bound::Unbounded,
                upper_bound: Bound::Excluded(upper),
                ..
            }) if col_id == lhs_col_id => {
                self.query.push(Query::IndexScan(IndexScan {
                    table,
                    col_id,
                    lower_bound: Self::bound(value, inclusive),
                    upper_bound: Bound::Excluded(upper),
                }));
                self
            }
            // merge with a preceding select
            Query::Select(filter) => {
                self.query.push(Query::Select(ColumnOp::Cmp {
                    op: OpQuery::Logic(OpLogic::And),
                    lhs: filter.into(),
                    rhs: IndexScan {
                        table,
                        col_id,
                        lower_bound: Self::bound(value, inclusive),
                        upper_bound: Bound::Unbounded,
                    }
                    .into(),
                }));
                self
            }
            // else generate a new select
            query => {
                self.query.push(query);
                self.query.push(Query::Select(
                    IndexScan {
                        table,
                        col_id,
                        lower_bound: Self::bound(value, inclusive),
                        upper_bound: Bound::Unbounded,
                    }
                    .into(),
                ));
                self
            }
        }
    }

    // Generate an index scan for a range predicate or try merging with a previous index scan.
    // Otherwise generate a select.
    // TODO: Replace these methods with a proper query optimization pass.
    pub fn with_index_upper_bound(
        mut self,
        table: DbTable,
        col_id: u32,
        value: AlgebraicValue,
        inclusive: bool,
    ) -> Self {
        // if this is the first operator in the list, generate an index scan
        let Some(query) = self.query.pop() else {
            self.query.push(Query::IndexScan(IndexScan {
                table,
                col_id,
                lower_bound: Bound::Unbounded,
                upper_bound: Self::bound(value, inclusive),
            }));
            return self;
        };
        match query {
            // try to push below join's lhs
            Query::JoinInner(JoinExpr {
                rhs:
                    QueryExpr {
                        source:
                            SourceExpr::DbTable(DbTable {
                                table_id: rhs_table_id, ..
                            }),
                        ..
                    },
                ..
            }) if table.table_id != rhs_table_id => {
                self = self.with_index_upper_bound(table, col_id, value, inclusive);
                self.query.push(query);
                self
            }
            // try to push below join's rhs
            Query::JoinInner(JoinExpr { rhs, col_lhs, col_rhs }) => {
                self.query.push(Query::JoinInner(JoinExpr {
                    rhs: rhs.with_index_upper_bound(table, col_id, value, inclusive),
                    col_lhs,
                    col_rhs,
                }));
                self
            }
            // merge with a preceding lower bounded index scan (inclusive)
            Query::IndexScan(IndexScan {
                col_id: lhs_col_id,
                lower_bound: Bound::Included(lower),
                upper_bound: Bound::Unbounded,
                ..
            }) if col_id == lhs_col_id => {
                self.query.push(Query::IndexScan(IndexScan {
                    table,
                    col_id,
                    lower_bound: Bound::Included(lower),
                    upper_bound: Self::bound(value, inclusive),
                }));
                self
            }
            // merge with a preceding lower bounded index scan (inclusive)
            Query::IndexScan(IndexScan {
                col_id: lhs_col_id,
                lower_bound: Bound::Excluded(lower),
                upper_bound: Bound::Unbounded,
                ..
            }) if col_id == lhs_col_id => {
                self.query.push(Query::IndexScan(IndexScan {
                    table,
                    col_id,
                    lower_bound: Bound::Excluded(lower),
                    upper_bound: Self::bound(value, inclusive),
                }));
                self
            }
            // merge with a preceding select
            Query::Select(filter) => {
                self.query.push(Query::Select(ColumnOp::Cmp {
                    op: OpQuery::Logic(OpLogic::And),
                    lhs: filter.into(),
                    rhs: IndexScan {
                        table,
                        col_id,
                        lower_bound: Bound::Unbounded,
                        upper_bound: Self::bound(value, inclusive),
                    }
                    .into(),
                }));
                self
            }
            // else generate a new select
            query => {
                self.query.push(query);
                self.query.push(Query::Select(
                    IndexScan {
                        table,
                        col_id,
                        lower_bound: Bound::Unbounded,
                        upper_bound: Self::bound(value, inclusive),
                    }
                    .into(),
                ));
                self
            }
        }
    }

    pub fn with_select<O>(mut self, op: O) -> Self
    where
        O: Into<ColumnOp>,
    {
        let Some(query) = self.query.pop() else {
            self.query.push(Query::Select(op.into()));
            return self;
        };

        match (query, op.into()) {
            (
                Query::JoinInner(JoinExpr { rhs, col_lhs, col_rhs }),
                ColumnOp::Cmp {
                    op: OpQuery::Cmp(cmp),
                    lhs: field,
                    rhs: value,
                },
            ) => match (*field, *value) {
                (ColumnOp::Field(FieldExpr::Name(field)), ColumnOp::Field(FieldExpr::Value(value)))
                    // Field is from lhs, so push onto join's left arg
                    if self.source.head().column(&field).is_some() =>
                {
                    self = self.with_select(ColumnOp::cmp(field, cmp, value));
                    self.query.push(Query::JoinInner(JoinExpr { rhs, col_lhs, col_rhs }));
                    self
                }
                (ColumnOp::Field(FieldExpr::Name(field)), ColumnOp::Field(FieldExpr::Value(value)))
                    // Field is from rhs, so push onto join's right arg
                    if rhs.source.head().column(&field).is_some() =>
                {
                    self.query.push(Query::JoinInner(JoinExpr {
                        rhs: rhs.with_select(ColumnOp::cmp(field, cmp, value)),
                        col_lhs,
                        col_rhs,
                    }));
                    self
                }
                (field, value) => {
                    self.query.push(Query::JoinInner(JoinExpr { rhs, col_lhs, col_rhs }));
                    self.query.push(Query::Select(ColumnOp::new(OpQuery::Cmp(cmp), field, value)));
                    self
                }
            },
            (Query::Select(filter), op) => {
                self.query
                    .push(Query::Select(ColumnOp::new(OpQuery::Logic(OpLogic::And), filter, op)));
                self
            }
            (query, op) => {
                self.query.push(query);
                self.query.push(Query::Select(op));
                self
            }
        }
    }

    pub fn with_select_cmp<LHS, RHS, O>(self, op: O, lhs: LHS, rhs: RHS) -> Self
    where
        LHS: Into<FieldExpr>,
        RHS: Into<FieldExpr>,
        O: Into<OpQuery>,
    {
        let op = ColumnOp::new(op.into(), ColumnOp::Field(lhs.into()), ColumnOp::Field(rhs.into()));
        self.with_select(op)
    }

    // Appends a project operation to the query operator pipeline.
    // The `wildcard_table_id` represents a projection of the form `table.*`.
    // This is used to determine if an inner join can be rewritten as an index join.
    pub fn with_project(self, cols: &[FieldExpr], wildcard_table_id: Option<u32>) -> Self {
        let mut x = self;
        if !cols.is_empty() {
            x.query.push(Query::Project(cols.into(), wildcard_table_id));
        }
        x
    }

    pub fn with_join_inner(self, with: impl Into<QueryExpr>, lhs: FieldName, rhs: FieldName) -> Self {
        let mut x = self;
        x.query.push(Query::JoinInner(JoinExpr::new(with.into(), lhs, rhs)));
        x
    }

    fn bound(value: AlgebraicValue, inclusive: bool) -> Bound<AlgebraicValue> {
        if inclusive {
            Bound::Included(value)
        } else {
            Bound::Excluded(value)
        }
    }
}

impl AuthAccess for Query {
    fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError> {
        if owner == caller {
            Ok(())
        } else if let Query::JoinInner(j) = self {
            if j.rhs.source.table_access() == StAccess::Public {
                Ok(())
            } else {
                Err(AuthError::TablePrivate {
                    named: j.rhs.source.table_name().to_string(),
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

#[derive(Debug, Clone, Eq, PartialEq, From)]
pub enum Expr {
    #[from]
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
        name: SatsString,
        columns: ProductTypeMeta,
        table_type: StTableType,
        table_access: StAccess,
    },
    Drop {
        name: SatsString,
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
                    let val = row.data.clone().into();
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
            Query::IndexScan(op) => {
                write!(f, "index_scan {:?}", op)
            }
            Query::IndexJoin(op) => {
                write!(f, "index_join {:?}", op)
            }
            Query::Select(q) => {
                write!(f, "select {q}")
            }
            Query::Project(q, _) => {
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
                write!(f, "&inner {:?} ON {} = {}", q.rhs, q.col_lhs, q.col_rhs)
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

impl From<QueryExpr> for QueryCode {
    fn from(value: QueryExpr) -> Self {
        QueryCode {
            table: value.source.into(),
            query: value.query,
        }
    }
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
        name: SatsString,
        columns: ProductTypeMeta,
        table_type: StTableType,
        table_access: StAccess,
    },
    Drop {
        name: SatsString,
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
