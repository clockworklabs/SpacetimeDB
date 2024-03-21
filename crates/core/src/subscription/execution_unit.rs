use super::query::{self, Supported};
use super::subscription::{IncrementalJoin, SupportedQuery};
use crate::db::relational_db::{RelationalDB, Tx};
use crate::error::DBError;
use crate::execution_context::ExecutionContext;
use crate::host::module_host::{DatabaseTableUpdate, DatabaseTableUpdateCow, TableOp, TableOpCow};
use crate::json::client_api::TableUpdateJson;
use crate::vm::{build_query, TxMode};
use spacetimedb_client_api_messages::client_api::{TableRowOperation, TableUpdate};
use spacetimedb_lib::ProductValue;
use spacetimedb_primitives::TableId;
use spacetimedb_sats::relation::DbTable;
use spacetimedb_vm::eval::IterRows;
use spacetimedb_vm::expr::{Query, QueryExpr, SourceExpr, SourceId, SourceSet};
use spacetimedb_vm::rel_ops::RelOps;
use spacetimedb_vm::relation::RelValue;
use std::hash::Hash;
use std::iter;

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
    /// which has a single source, an in-memory table, produced by [`query::query_to_mem_table`].
    Select(QueryExpr),
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
    eval_plan: QueryExpr,
    /// A version of the plan optimized for `eval_incr`,
    /// whose source is an in-memory table, as if by [`query::to_mem_table`].
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
    /// Pre-compute a plan for `eval_incr` which reads from an in-memory table
    /// rather than re-planning on every incremental update.
    fn compile_select_eval_incr(expr: &QueryExpr) -> QueryExpr {
        let source = &expr.source;
        assert!(
            source.is_db_table(),
            "The plan passed to `compile_select_eval_incr` must read from `DbTable`s, but found in-mem table"
        );
        // NOTE: The `eval_incr_plan` will reference a `SourceExpr::InMemory`
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
        let source = SourceExpr::from_mem_table(source.head().clone(), source.table_access(), 0, SourceId(0));
        let query = expr.query.clone();
        QueryExpr { source, query }
    }

    pub fn new(eval_plan: SupportedQuery, hash: QueryHash) -> Result<Self, DBError> {
        // Pre-compile the `expr` as fully as possible, twice, for two different paths:
        // - `eval_incr_plan`, for incremental updates from an `SourceExpr::InMemory` table.
        // - `eval_plan`, for initial subscriptions from a `SourceExpr::DbTable`.

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
        let eval_plan = eval_plan.expr;
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
            .source
            .get_db_table()
            .expect("ExecutionUnit eval_plan should have DbTable source, but found in-mem table")
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

    /// Evaluate this execution unit against the database using the json format.
    #[tracing::instrument(skip_all)]
    pub fn eval_json(&self, db: &RelationalDB, tx: &Tx) -> Result<Option<TableUpdateJson>, DBError> {
        let table_row_operations = Self::eval_query_expr(db, tx, &self.eval_plan, |row| {
            TableOp::insert(row.into_product_value()).into()
        })?;
        Ok((!table_row_operations.is_empty()).then(|| TableUpdateJson {
            table_id: self.return_table().into(),
            table_name: self.return_name(),
            table_row_operations,
        }))
    }

    /// Evaluate this execution unit against the database using the binary format.
    #[tracing::instrument(skip_all)]
    pub fn eval_binary(&self, db: &RelationalDB, tx: &Tx) -> Result<Option<TableUpdate>, DBError> {
        let mut buf = Vec::new();
        let table_row_operations = Self::eval_query_expr(db, tx, &self.eval_plan, |row| {
            row.to_bsatn_extend(&mut buf).unwrap();
            let row = buf.clone();
            buf.clear();
            TableRowOperation { op: 1, row }
        })?;
        Ok((!table_row_operations.is_empty()).then(|| TableUpdate {
            table_id: self.return_table().into(),
            table_name: self.return_name(),
            table_row_operations,
        }))
    }

    fn eval_query_expr<T>(
        db: &RelationalDB,
        tx: &Tx,
        eval_plan: &QueryExpr,
        convert: impl FnMut(RelValue<'_>) -> T,
    ) -> Result<Vec<T>, DBError> {
        let ctx = ExecutionContext::subscribe(db.address());
        let tx: TxMode = tx.into();
        let query = build_query::<iter::Empty<_>>(&ctx, db, &tx, eval_plan, &mut |_| None)?;
        let ops = query.collect_vec(convert)?;
        Ok(ops)
    }

    /// Evaluate this execution unit against the given delta tables.
    pub fn eval_incr<'a>(
        &'a self,
        ctx: &'a ExecutionContext<'a>,
        db: &'a RelationalDB,
        tx: &'a TxMode<'a>,
        tables: impl Iterator<Item = &'a DatabaseTableUpdate>,
    ) -> Result<Option<DatabaseTableUpdateCow<'a>>, DBError> {
        let ops = match &self.eval_incr_plan {
            EvalIncrPlan::Select(eval_incr_plan) => {
                Self::eval_incr_query_expr(ctx, db, tx, tables, eval_incr_plan, self.return_table())?
            }
            EvalIncrPlan::Semijoin(eval_incr_plan) => eval_incr_plan
                .eval(ctx, db, tx, tables)?
                .map(Vec::<_>::from_iter)
                .unwrap_or_default(),
        };
        Ok((!ops.is_empty()).then(|| DatabaseTableUpdateCow {
            table_id: self.return_table(),
            table_name: self.return_name(),
            ops,
        }))
    }

    fn eval_query_expr_against_memtable<'a>(
        ctx: &'a ExecutionContext,
        db: &'a RelationalDB,
        tx: &'a TxMode,
        mem_table: Vec<&'a ProductValue>,
        eval_incr_plan: &'a QueryExpr,
    ) -> Result<Box<IterRows<'a>>, DBError> {
        // Build a `SourceSet` containing the updates from `table`.
        let mut sources: SourceSet<_, 1> = [mem_table.into_iter().map(RelValue::ProjRef)].into();
        // Evaluate the saved plan against the new `SourceSet`,
        // returning an iterator over the selected rows.
        build_query(ctx, db, tx, eval_incr_plan, &mut |id| sources.take(id)).map_err(Into::into)
    }

    fn eval_incr_query_expr<'a>(
        ctx: &'a ExecutionContext<'a>,
        db: &'a RelationalDB,
        tx: &'a TxMode<'a>,
        tables: impl Iterator<Item = &'a DatabaseTableUpdate>,
        eval_incr_plan: &'a QueryExpr,
        return_table: TableId,
    ) -> Result<Vec<TableOpCow<'a>>, DBError> {
        assert!(
            eval_incr_plan.source.is_mem_table(),
            "Expected in-mem table in `eval_incr_plan`, but found `DbTable`"
        );

        // Partition the `update` into two `MemTable`s, `(inserts, deletes)`,
        // so that we can remember which are which without adding a column to each row.
        // Previously, we used to add such a column `"__op_type: AlgebraicType::U8"`.
        fn partition_updates(update: &DatabaseTableUpdate) -> (Option<Vec<&ProductValue>>, Option<Vec<&ProductValue>>) {
            // Pre-allocate with capacity given by an upper bound,
            // because realloc is worse than over-allocing.
            let mut inserts = Vec::with_capacity(update.ops.len());
            let mut deletes = Vec::with_capacity(update.ops.len());
            for op in update.ops.iter() {
                // 0 = delete, 1 = insert
                if op.op_type == 0 { &mut deletes } else { &mut inserts }.push(&op.row);
            }
            (
                (!inserts.is_empty()).then_some(inserts),
                (!deletes.is_empty()).then_some(deletes),
            )
        }

        let mut ops = Vec::new();

        for table in tables.filter(|table| table.table_id == return_table) {
            // Evaluate the query separately against inserts and deletes,
            // so that we can pass each row to the query engine unaltered,
            // without forgetting which are inserts and which are deletes.
            // Then, collect the rows into the single `ops` vec,
            // restoring the appropriate `op_type`.
            let (inserts, deletes) = partition_updates(table);
            if let Some(inserts) = inserts {
                let query = Self::eval_query_expr_against_memtable(ctx, db, tx, inserts, eval_incr_plan)?;
                // op_type 1: insert
                Self::collect_rows_with_table_op(&mut ops, query, 1)?;
            }
            if let Some(deletes) = deletes {
                let query = Self::eval_query_expr_against_memtable(ctx, db, tx, deletes, eval_incr_plan)?;
                // op_type 0: delete
                Self::collect_rows_with_table_op(&mut ops, query, 0)?;
            }
        }
        Ok(ops)
    }

    /// Collect the results of `query` into a vec `into`,
    /// annotating each as a `TableOp` with the `op_type`.
    fn collect_rows_with_table_op<'a>(
        into: &mut Vec<TableOpCow<'a>>,
        mut query: Box<IterRows<'a>>,
        op_type: u8,
    ) -> Result<(), DBError> {
        while let Some(row) = query.next()? {
            let row = row.into_product_value_cow();
            into.push(TableOpCow { op_type, row });
        }
        Ok(())
    }
}
