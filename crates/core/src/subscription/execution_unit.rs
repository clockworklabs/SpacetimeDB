use super::query::{self, Supported};
use super::subscription::{IncrementalJoin, SupportedQuery};
use crate::db::datastore::locking_tx_datastore::tx::TxId;
use crate::db::relational_db::{RelationalDB, Tx};
use crate::error::DBError;
use crate::estimation;
use crate::execution_context::ExecutionContext;
use crate::host::module_host::{
    rel_value_to_table_row_op_binary, rel_value_to_table_row_op_json, DatabaseTableUpdate, DatabaseTableUpdateRelValue,
    OpType, UpdatesRelValue,
};
use crate::json::client_api::TableUpdateJson;
use crate::util::slow::SlowQueryLogger;
use crate::vm::{build_query, TxMode};
use spacetimedb_client_api_messages::client_api::TableUpdate;
use spacetimedb_lib::{Identity, ProductValue};
use spacetimedb_primitives::TableId;
use spacetimedb_sats::db::error::AuthError;
use spacetimedb_sats::relation::DbTable;
use spacetimedb_vm::eval::IterRows;
use spacetimedb_vm::expr::{AuthAccess, NoInMemUsed, Query, QueryExpr, SourceExpr, SourceId};
use spacetimedb_vm::rel_ops::RelOps;
use spacetimedb_vm::relation::RelValue;
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

    pub(crate) sql: String,
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
        let source = SourceExpr::from_mem_table(source.head().clone(), source.table_access(), SourceId(0));
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
                ..
            } => EvalIncrPlan::Select(Self::compile_select_eval_incr(expr)),
            SupportedQuery {
                kind: query::Supported::Semijoin,
                expr,
                ..
            } => EvalIncrPlan::Semijoin(IncrementalJoin::new(expr)?),
        };
        Ok(ExecutionUnit {
            hash,
            sql: eval_plan.sql,
            eval_plan: eval_plan.expr,
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

    pub fn return_name(&self) -> Box<str> {
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
    pub fn eval_json(&self, ctx: &ExecutionContext, db: &RelationalDB, tx: &Tx, sql: &str) -> Option<TableUpdateJson> {
        let table_row_operations = Self::eval_query_expr(ctx, db, tx, &self.eval_plan, sql, |row| {
            rel_value_to_table_row_op_json(row, OpType::Insert)
        });
        (!table_row_operations.is_empty()).then(|| TableUpdateJson {
            table_id: self.return_table().into(),
            table_name: self.return_name(),
            table_row_operations,
        })
    }

    /// Evaluate this execution unit against the database using the binary format.
    #[tracing::instrument(skip_all)]
    pub fn eval_binary(&self, ctx: &ExecutionContext, db: &RelationalDB, tx: &Tx, sql: &str) -> Option<TableUpdate> {
        let mut scratch = Vec::new();
        let table_row_operations = Self::eval_query_expr(ctx, db, tx, &self.eval_plan, sql, |row| {
            rel_value_to_table_row_op_binary(&mut scratch, &row, OpType::Insert)
        });
        (!table_row_operations.is_empty()).then(|| TableUpdate {
            table_id: self.return_table().into(),
            table_name: self.return_name().into(),
            table_row_operations,
        })
    }

    fn eval_query_expr<T>(
        ctx: &ExecutionContext,
        db: &RelationalDB,
        tx: &Tx,
        eval_plan: &QueryExpr,
        sql: &str,
        convert: impl FnMut(RelValue<'_>) -> T,
    ) -> Vec<T> {
        let _slow_query = SlowQueryLogger::subscription(ctx, sql).log_guard();
        build_query(ctx, db, &tx.into(), eval_plan, &mut NoInMemUsed).collect_vec(convert)
    }

    /// Evaluate this execution unit against the given delta tables.
    pub fn eval_incr<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        db: &'a RelationalDB,
        tx: &'a TxMode<'a>,
        sql: &'a str,
        tables: impl 'a + Clone + Iterator<Item = &'a DatabaseTableUpdate>,
    ) -> Option<DatabaseTableUpdateRelValue<'a>> {
        let _slow_query = SlowQueryLogger::incremental_updates(ctx, sql).log_guard();
        let updates = match &self.eval_incr_plan {
            EvalIncrPlan::Select(plan) => Self::eval_incr_query_expr(ctx, db, tx, tables, plan, self.return_table()),
            EvalIncrPlan::Semijoin(plan) => plan.eval(ctx, db, tx, tables),
        };

        updates.has_updates().then(|| DatabaseTableUpdateRelValue {
            table_id: self.return_table(),
            table_name: self.return_name(),
            updates,
        })
    }

    fn eval_query_expr_against_memtable<'a>(
        ctx: &'a ExecutionContext,
        db: &'a RelationalDB,
        tx: &'a TxMode,
        mem_table: &'a [ProductValue],
        eval_incr_plan: &'a QueryExpr,
    ) -> Box<IterRows<'a>> {
        // Provide the updates from `table`.
        let sources = &mut Some(mem_table.iter().map(RelValue::ProjRef));
        // Evaluate the saved plan against the new updates,
        // returning an iterator over the selected rows.
        build_query(ctx, db, tx, eval_incr_plan, sources)
    }

    fn eval_incr_query_expr<'a>(
        ctx: &'a ExecutionContext,
        db: &'a RelationalDB,
        tx: &'a TxMode<'a>,
        tables: impl Iterator<Item = &'a DatabaseTableUpdate>,
        eval_incr_plan: &'a QueryExpr,
        return_table: TableId,
    ) -> UpdatesRelValue<'a> {
        assert!(
            eval_incr_plan.source.is_mem_table(),
            "Expected in-mem table in `eval_incr_plan`, but found `DbTable`"
        );

        let mut deletes = Vec::new();
        let mut inserts = Vec::new();
        for table in tables.filter(|table| table.table_id == return_table) {
            // Evaluate the query separately against inserts and deletes,
            // so that we can pass each row to the query engine unaltered,
            // without forgetting which are inserts and which are deletes.
            // Previously, we used to add such a column `"__op_type: AlgebraicType::U8"`.
            if !table.inserts.is_empty() {
                let query = Self::eval_query_expr_against_memtable(ctx, db, tx, &table.inserts, eval_incr_plan);
                Self::collect_rows(&mut inserts, query);
            }
            if !table.deletes.is_empty() {
                let query = Self::eval_query_expr_against_memtable(ctx, db, tx, &table.deletes, eval_incr_plan);
                Self::collect_rows(&mut deletes, query);
            }
        }
        UpdatesRelValue { deletes, inserts }
    }

    /// Collect the results of `query` into a vec `sink`.
    fn collect_rows<'a>(sink: &mut Vec<RelValue<'a>>, mut query: Box<IterRows<'a>>) {
        while let Some(row) = query.next() {
            sink.push(row);
        }
    }

    /// The estimated number of rows returned by this execution unit.
    pub fn row_estimate(&self, tx: &TxId) -> u64 {
        estimation::num_rows(tx, &self.eval_plan)
    }
}

impl AuthAccess for ExecutionUnit {
    fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError> {
        self.eval_plan.check_auth(owner, caller)
    }
}
