use super::query::{self, run_query, Supported};
use super::subscription::{eval_primary_updates, IncrementalJoin, SupportedQuery};
use crate::db::relational_db::{RelationalDB, Tx};
use crate::error::DBError;
use crate::execution_context::ExecutionContext;
use crate::host::module_host::{DatabaseTableUpdate, TableOp};
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_primitives::TableId;
use spacetimedb_vm::expr::{QueryExpr, SourceExpr, SourceSet};
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

/// An atomic unit of execution within a subscription set.
/// Currently just a single query plan,
/// however in the future this could be multiple query plans,
/// such as those of an incremental join.
#[derive(Debug)]
pub struct ExecutionUnit {
    hash: QueryHash,
    eval_plan: SupportedQuery,

    /// For single-table selects, a version of the plan optimized for `eval_incr`,
    /// whose source is a [`MemTable`], as if by [`query::to_mem_table`].
    ///
    /// This will be paired with a [`SourceSet`] of one element,
    /// a `MemTable` of row updates, as produced by [`query::to_mem_table_with_op_type`].
    ///
    /// Currently `None` for joins.
    /// TODO(perf): Re-use query plans for incremental joins.
    eval_incr_plan: Option<QueryExpr>,
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
        Self::new(plan, QueryHash::NONE)
    }
}

impl ExecutionUnit {
    pub fn new(eval_plan: SupportedQuery, hash: QueryHash) -> Self {
        let eval_incr_plan = match &eval_plan {
            SupportedQuery {
                kind: query::Supported::Select,
                expr,
            } => {
                // Pre-compute a plan for `eval_incr` which reads from a `MemTable`
                // whose rows are augmented with an `__op_type` column,
                // rather than re-planning on every incremental update.

                let source = expr.source.get_db_table().expect(
                    "The plan passed to `ExecutionUnit::new` must read from `DbTable`s, but found a `MemTable`",
                );
                let table_id = source.table_id;
                let table_name = source.head.table_name.clone();
                let table_update = DatabaseTableUpdate {
                    table_id,
                    table_name,
                    ops: vec![],
                };
                let expr = expr.clone();
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
                let (eval_incr_plan, _source_set) = query::to_mem_table(expr, &table_update);
                debug_assert_eq!(_source_set.len(), 1);
                Some(eval_incr_plan)
            }
            SupportedQuery {
                kind: query::Supported::Semijoin,
                expr: _expr,
            } => None,
        };
        ExecutionUnit {
            hash,
            eval_plan,
            eval_incr_plan,
        }
    }

    /// Is this a single table select or a semijoin?
    pub fn kind(&self) -> Supported {
        self.eval_plan.kind
    }

    /// The unique query hash for this execution unit.
    pub fn hash(&self) -> QueryHash {
        self.hash
    }

    /// The table from which this query returns rows.
    pub fn return_table(&self) -> TableId {
        self.eval_plan.return_table()
    }

    pub fn return_name(&self) -> String {
        self.eval_plan.return_name()
    }

    /// The table on which this query filters rows.
    /// In the case of a single table select,
    /// this is the same as the return table.
    /// In the case of a semijoin,
    /// it is the auxiliary table against which we are joining.
    pub fn filter_table(&self) -> TableId {
        self.eval_plan.filter_table()
    }

    /// Evaluate this execution unit against the database.
    #[tracing::instrument(skip_all)]
    pub fn eval(&self, db: &RelationalDB, tx: &Tx, auth: AuthCtx) -> Result<Option<DatabaseTableUpdate>, DBError> {
        let ctx = ExecutionContext::subscribe(db.address());
        let mut ops = vec![];
        for table in run_query(&ctx, db, tx, &self.eval_plan.expr, auth, SourceSet::default())? {
            ops.extend(table.data.into_iter().map(TableOp::insert));
        }
        Ok((!ops.is_empty()).then(|| DatabaseTableUpdate {
            table_id: self.return_table(),
            table_name: self.return_name(),
            ops,
        }))
    }

    /// Evaluate this execution unit against the given delta tables.
    #[tracing::instrument(skip_all)]
    pub fn eval_incr<'a>(
        &'a self,
        db: &RelationalDB,
        tx: &Tx,
        tables: impl Iterator<Item = &'a DatabaseTableUpdate>,
        auth: AuthCtx,
    ) -> Result<Option<DatabaseTableUpdate>, DBError> {
        let ops = if let Some(eval_incr_plan) = &self.eval_incr_plan {
            debug_assert!(matches!(self.eval_plan.kind, Supported::Select));
            let SourceExpr::MemTable {
                source_id: _source_id,
                ref header,
                table_access,
                ..
            } = eval_incr_plan.source
            else {
                panic!("Expected MemTable in `eval_incr_plan`, but found `DbTable`");
            };
            let mut ops = Vec::new();
            for table in tables.filter(|table| table.table_id == self.return_table()) {
                // Build a `SourceSet` containing the updates from `table`.
                let mem_table = query::to_mem_table_with_op_type(header.clone(), table_access, table);
                let mut sources = SourceSet::default();
                let _source_expr = sources.add_mem_table(mem_table);
                debug_assert_eq!(_source_expr.source_id(), Some(_source_id));
                // Evaluate the saved plan against the new `SourceSet`
                // and capture the new row operations.
                ops.extend(
                    eval_primary_updates(db, auth, tx, eval_incr_plan, sources)?.map(|r| TableOp::new(r.0, r.1)),
                );
            }
            ops
        } else {
            debug_assert!(matches!(self.eval_plan.kind, Supported::Semijoin));
            if let Some(plan) = IncrementalJoin::new(&self.eval_plan.expr, tables.into_iter())? {
                // Evaluate the plan and capture the new row operations
                plan.eval(db, tx, &auth)?.collect()
            } else {
                vec![]
            }
        };
        Ok((!ops.is_empty()).then(|| DatabaseTableUpdate {
            table_id: self.return_table(),
            table_name: self.return_name(),
            ops,
        }))
    }
}
