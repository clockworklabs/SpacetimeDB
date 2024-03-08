use super::query::{self, find_op_type_col_pos, Supported, OP_TYPE_FIELD_NAME};
use super::subscription::{IncrementalJoin, SupportedQuery};
use crate::db::relational_db::{RelationalDB, Tx};
use crate::error::DBError;
use crate::execution_context::ExecutionContext;
use crate::host::module_host::{DatabaseTableUpdate, TableOp};
use crate::vm::{build_query, TxMode};
use spacetimedb_primitives::TableId;
use spacetimedb_sats::relation::{DbTable, Header};
use spacetimedb_vm::eval::IterRows;
use spacetimedb_vm::expr::{Query, QueryCode, QueryExpr, SourceExpr, SourceSet};
use spacetimedb_vm::rel_ops::RelOps;
use std::hash::Hash;

/// A hash for uniquely identifying query execution units,
/// to avoid recompilation of queries that have an open subscription.
///
/// Currently we are using a cryptographic hash,
/// which is most certainly overkill.
/// However the benefits include uniqueness by definition,
/// and a compact representation for equality comparisons.
///
/// It also decouples the hash from the physical plan.
///
/// Note that we could hash QueryExprs directly,
/// using the standard library's hasher.
/// However some execution units are comprised of several query plans,
/// as is the case for incremental joins.
/// And we want to associate a hash with the entire unit of execution,
/// rather than an individual plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QueryHash {
    data: [u8; 32],
}

impl QueryHash {
    pub const NONE: Self = Self { data: [0; 32] };

    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            data: blake3::hash(bytes).into(),
        }
    }

    pub fn from_string(str: &str) -> Self {
        Self::from_bytes(str.as_bytes())
    }
}

#[derive(Debug)]
enum EvalIncrPlan {
    /// For semijoins, store several versions of the plan,
    /// for querying all combinations of L_{inserts/deletes/committed} * R_(inserts/deletes/committed).
    Semijoin(IncrementalJoin),

    /// For single-table selects, store only one version of the plan,
    /// which has a single source, a [`MemTable`] produced by [`query::to_mem_table_with_op_type`].
    Select(QueryCode),
}

/// An atomic unit of execution within a subscription set.
/// Currently just a single query plan,
/// however in the future this could be multiple query plans,
/// such as those of an incremental join.
#[derive(Debug)]
pub struct ExecutionUnit {
    hash: QueryHash,

    /// A version of the plan optimized for `eval`,
    /// whose source is a [`DbTable`].
    ///
    /// This is a direct compilation of the source query.
    eval_plan: QueryCode,
    /// A version of the plan optimized for `eval_incr`,
    /// whose source is a [`MemTable`], as if by [`query::to_mem_table`].
    eval_incr_plan: EvalIncrPlan,
}

/// An ExecutionUnit is uniquely identified by its QueryHash.
impl Eq for ExecutionUnit {}

impl PartialEq for ExecutionUnit {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl From<SupportedQuery> for ExecutionUnit {
    // Used in tests and benches.
    // TODO(bikeshedding): Remove this impl,
    // in favor of more explcit calls to `ExecutionUnit::new` with `QueryHash::NONE`.
    fn from(plan: SupportedQuery) -> Self {
        Self::new(plan, QueryHash::NONE).unwrap()
    }
}

impl ExecutionUnit {
    fn compile_query_expr_to_query_code(expr: QueryExpr) -> QueryCode {
        spacetimedb_vm::eval::compile_query(expr)
    }

    /// Pre-compute a plan for `eval_incr` which reads from a `MemTable`
    /// whose rows are augmented with an `__op_type` column,
    /// rather than re-planning on every incremental update.
    fn compile_select_eval_incr(expr: &QueryExpr) -> QueryCode {
        let source = expr
            .source
            .get_db_table()
            .expect("The plan passed to `ExecutionUnit::new` must read from `DbTable`s, but found a `MemTable`");
        let table_id = source.table_id;
        let table_name = source.head.table_name.clone();
        let table_update = DatabaseTableUpdate {
            table_id,
            table_name,
            ops: vec![],
        };

        // NOTE: The `eval_incr_plan` will reference a `SourceExpr::MemTable`
        // with `row_count: RowCount::exact(0)`.
        // This is inaccurate; while we cannot predict the exact number of rows,
        // we know that it will never be 0,
        // as we wouldn't have a [`DatabaseTableUpdate`] with no changes.
        //
        // Our current query planner doesn't use the `row_count` in any meaningful way,
        // so this is fine.
        // Some day down the line, when we have a real query planner,
        // we may need to provide a row count estimation that is, if not accurate,
        // at least less specifically inaccurate.
        let (eval_incr_plan, _source_set) = query::to_mem_table(expr.clone(), &table_update);
        debug_assert_eq!(_source_set.len(), 1);

        Self::compile_query_expr_to_query_code(eval_incr_plan)
    }

    fn compile_eval(expr: QueryExpr) -> QueryCode {
        Self::compile_query_expr_to_query_code(expr)
    }

    pub fn new(eval_plan: SupportedQuery, hash: QueryHash) -> Result<Self, DBError> {
        // Pre-compile the `expr` as fully as possible, twice, for two different paths:
        // - `eval_incr_plan`, for incremental updates from a `MemTable`.
        // - `eval_plan`, for initial subscriptions from a `DbTable`.

        let eval_incr_plan = match &eval_plan {
            SupportedQuery {
                kind: query::Supported::Select,
                expr,
            } => EvalIncrPlan::Select(Self::compile_select_eval_incr(expr)),
            SupportedQuery {
                kind: query::Supported::Semijoin,
                expr,
            } => EvalIncrPlan::Semijoin(IncrementalJoin::new(expr)?),
        };
        let eval_plan = Self::compile_eval(eval_plan.expr);
        Ok(ExecutionUnit {
            hash,
            eval_plan,
            eval_incr_plan,
        })
    }

    /// Is this a single table select or a semijoin?
    pub fn kind(&self) -> Supported {
        match self.eval_incr_plan {
            EvalIncrPlan::Select(_) => Supported::Select,
            EvalIncrPlan::Semijoin(_) => Supported::Semijoin,
        }
    }

    /// The unique query hash for this execution unit.
    pub fn hash(&self) -> QueryHash {
        self.hash
    }

    fn return_db_table(&self) -> &DbTable {
        self.eval_plan
            .table
            .get_db_table()
            .expect("ExecutionUnit eval_plan should have DbTable source, but found MemTable")
    }

    /// The table from which this query returns rows.
    pub fn return_table(&self) -> TableId {
        self.return_db_table().table_id
    }

    pub fn return_name(&self) -> String {
        self.return_db_table().head.table_name.clone()
    }

    /// The table on which this query filters rows.
    /// In the case of a single table select,
    /// this is the same as the return table.
    /// In the case of a semijoin,
    /// it is the auxiliary table against which we are joining.
    pub fn filter_table(&self) -> TableId {
        let return_table = self.return_table();
        self.eval_plan
            .query
            .first()
            .and_then(|op| {
                if let Query::IndexJoin(join) = op {
                    Some(join)
                } else {
                    None
                }
            })
            .and_then(|join| {
                join.index_side
                    .get_db_table()
                    .filter(|t| t.table_id != return_table)
                    .or_else(|| join.probe_side.source.get_db_table())
                    .filter(|t| t.table_id != return_table)
                    .map(|t| t.table_id)
            })
            .unwrap_or(return_table)
    }

    /// Evaluate this execution unit against the database.
    pub fn eval(&self, db: &RelationalDB, tx: &Tx) -> Result<Option<DatabaseTableUpdate>, DBError> {
        let ops = Self::eval_query_code(db, tx, &self.eval_plan)?;
        Ok((!ops.is_empty()).then(|| DatabaseTableUpdate {
            table_id: self.return_table(),
            table_name: self.return_name(),
            ops,
        }))
    }

    fn eval_query_code(db: &RelationalDB, tx: &Tx, eval_plan: &QueryCode) -> Result<Vec<TableOp>, DBError> {
        let ctx = ExecutionContext::subscribe(db.address());
        let tx: TxMode = tx.into();
        // TODO(perf, 833): avoid clone.
        let query = build_query(&ctx, db, &tx, eval_plan.clone(), &mut SourceSet::default())?;
        let ops = query.collect_vec(|row_ref| TableOp::insert(row_ref.into_product_value()))?;
        Ok(ops)
    }

    /// Evaluate this execution unit against the given delta tables.
    pub fn eval_incr<'a>(
        &'a self,
        db: &RelationalDB,
        tx: &Tx,
        tables: impl Iterator<Item = &'a DatabaseTableUpdate>,
    ) -> Result<Option<DatabaseTableUpdate>, DBError> {
        let ops = match &self.eval_incr_plan {
            EvalIncrPlan::Select(eval_incr_plan) => {
                Self::eval_incr_query_code(db, tx, tables, eval_incr_plan, self.return_table())?
            }
            EvalIncrPlan::Semijoin(eval_incr_plan) => eval_incr_plan
                .eval(db, tx, tables)?
                .map(<Vec<_>>::from_iter)
                .unwrap_or(vec![]),
        };
        Ok((!ops.is_empty()).then(|| DatabaseTableUpdate {
            table_id: self.return_table(),
            table_name: self.return_name(),
            ops,
        }))
    }

    fn eval_incr_query_code<'a>(
        db: &RelationalDB,
        tx: &Tx,
        tables: impl Iterator<Item = &'a DatabaseTableUpdate>,
        eval_incr_plan: &QueryCode,
        return_table: TableId,
    ) -> Result<Vec<TableOp>, DBError> {
        let ctx = ExecutionContext::incremental_update(db.address());
        let tx: TxMode = tx.into();

        let SourceExpr::MemTable {
            source_id: _source_id,
            ref header,
            table_access,
            ..
        } = eval_incr_plan.table
        else {
            panic!("Expected MemTable in `eval_incr_plan`, but found `DbTable`");
        };
        let mut ops = Vec::new();

        for table in tables.filter(|table| table.table_id == return_table) {
            // Build a `SourceSet` containing the updates from `table`.
            let mem_table = query::to_mem_table_with_op_type(header.clone(), table_access, table);
            let mut sources = SourceSet::default();
            let _source_expr = sources.add_mem_table(mem_table);
            debug_assert_eq!(_source_expr.source_id(), Some(_source_id));
            // Evaluate the saved plan against the new `SourceSet`
            // and capture the new row operations.
            // TODO(perf, 833): avoid clone.
            let query = build_query(&ctx, db, &tx, eval_incr_plan.clone(), &mut sources)?;
            Self::collect_rows_remove_table_ops(&mut ops, query, header)?;
        }
        Ok(ops)
    }

    /// Convert a set of rows annotated with the `__op_type` fields into a set of [`TableOp`]s,
    /// and collect them into a vec `into`.
    fn collect_rows_remove_table_ops(
        into: &mut Vec<TableOp>,
        mut query: Box<IterRows<'_>>,
        header: &Header,
    ) -> Result<(), DBError> {
        let pos_op_type = find_op_type_col_pos(header).unwrap_or_else(|| {
            panic!(
                "Failed to locate `{OP_TYPE_FIELD_NAME}` in `{}`, fields: {:?}",
                header.table_name,
                header.fields.iter().map(|x| &x.field).collect::<Vec<_>>()
            )
        });
        let pos_op_type = pos_op_type.idx();
        while let Some(row_ref) = query.next()? {
            let mut row = row_ref.into_product_value();
            let op_type =
                row.elements.remove(pos_op_type).into_u8().unwrap_or_else(|_| {
                    panic!("Failed to extract `{OP_TYPE_FIELD_NAME}` from `{}`", header.table_name)
                });
            into.push(TableOp::new(op_type, row));
        }
        Ok(())
    }
}
