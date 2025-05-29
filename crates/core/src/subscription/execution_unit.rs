use super::query::{self, Supported};
use super::subscription::{IncrementalJoin, SupportedQuery};
use crate::db::datastore::locking_tx_datastore::tx::TxId;
use crate::db::relational_db::{RelationalDB, Tx};
use crate::error::DBError;
use crate::estimation;
use crate::host::module_host::{DatabaseTableUpdate, DatabaseTableUpdateRelValue, UpdatesRelValue};
use crate::messages::websocket::TableUpdate;
use crate::util::slow::SlowQueryLogger;
use crate::vm::{build_query, TxMode};
use spacetimedb_client_api_messages::websocket::{
    Compression, QueryUpdate, RowListLen as _, SingleQueryUpdate, WebsocketFormat,
};
use spacetimedb_lib::db::error::AuthError;
use spacetimedb_lib::relation::DbTable;
use spacetimedb_lib::{Identity, ProductValue};
use spacetimedb_primitives::TableId;
use spacetimedb_sats::u256;
use spacetimedb_vm::eval::IterRows;
use spacetimedb_vm::expr::{AuthAccess, NoInMemUsed, Query, QueryExpr, SourceExpr, SourceId};
use spacetimedb_vm::rel_ops::RelOps;
use spacetimedb_vm::relation::RelValue;
use std::hash::Hash;
use std::time::Duration;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QueryHash {
    data: [u8; 32],
}

impl From<QueryHash> for u256 {
    fn from(hash: QueryHash) -> Self {
        u256::from_le_bytes(hash.data)
    }
}

impl QueryHash {
    /// The zero value of a QueryHash
    pub const NONE: Self = Self { data: [0; 32] };

    /// The min value of a QueryHash
    pub const MIN: Self = Self::NONE;

    /// The max value of a QueryHash
    pub const MAX: Self = Self { data: [0xFFu8; 32] };

    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            data: blake3::hash(bytes).into(),
        }
    }

    /// Generate a hash from a query string
    pub fn from_string(str: &str, identity: Identity, has_param: bool) -> Self {
        if has_param {
            return Self::from_string_and_identity(str, identity);
        }
        Self::from_bytes(str.as_bytes())
    }

    /// If a query is parameterized with `:sender`, we must use the value of `:sender`,
    /// i.e. the identity of the caller, when hashing the query text,
    /// so that two identical queries from different clients aren't hashed to the same value.
    ///
    /// TODO: Once we have RLS, this hash must computed after name resolution.
    /// It can no longer be computed from the source text.
    pub fn from_string_and_identity(str: &str, identity: Identity) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(str.as_bytes());
        hasher.update(&identity.to_byte_array());
        Self {
            data: hasher.finalize().into(),
        }
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

    /// Evaluate this execution unit against the database using the specified format.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn eval<F: WebsocketFormat>(
        &self,
        db: &RelationalDB,
        tx: &Tx,
        sql: &str,
        slow_query_threshold: Option<Duration>,
        compression: Compression,
    ) -> Option<TableUpdate<F>> {
        let _slow_query = SlowQueryLogger::new(sql, slow_query_threshold, tx.ctx.workload()).log_guard();

        // Build & execute the query and then encode it to a row list.
        let tx = &tx.into();
        let mut inserts = build_query(db, tx, &self.eval_plan, &mut NoInMemUsed);
        let inserts = inserts.iter();
        let (inserts, num_rows) = F::encode_list(inserts);

        (!inserts.is_empty()).then(|| {
            let deletes = F::List::default();
            let qu = QueryUpdate { deletes, inserts };
            let update = F::into_query_update(qu, compression);
            TableUpdate::new(
                self.return_table(),
                self.return_name(),
                SingleQueryUpdate { update, num_rows },
            )
        })
    }

    /// Evaluate this execution unit against the given delta tables.
    pub fn eval_incr<'a>(
        &'a self,
        db: &'a RelationalDB,
        tx: &'a TxMode<'a>,
        sql: &'a str,
        tables: impl 'a + Clone + Iterator<Item = &'a DatabaseTableUpdate>,
        slow_query_threshold: Option<Duration>,
    ) -> Option<DatabaseTableUpdateRelValue<'a>> {
        let _slow_query = SlowQueryLogger::new(sql, slow_query_threshold, tx.ctx().workload()).log_guard();
        let updates = match &self.eval_incr_plan {
            EvalIncrPlan::Select(plan) => Self::eval_incr_query_expr(db, tx, tables, plan, self.return_table()),
            EvalIncrPlan::Semijoin(plan) => plan.eval(db, tx, tables),
        };

        updates.has_updates().then(|| DatabaseTableUpdateRelValue {
            table_id: self.return_table(),
            table_name: self.return_name(),
            updates,
        })
    }

    fn eval_query_expr_against_memtable<'a>(
        db: &'a RelationalDB,
        tx: &'a TxMode,
        mem_table: &'a [ProductValue],
        eval_incr_plan: &'a QueryExpr,
    ) -> Box<IterRows<'a>> {
        // Provide the updates from `table`.
        let sources = &mut Some(mem_table.iter().map(RelValue::ProjRef));
        // Evaluate the saved plan against the new updates,
        // returning an iterator over the selected rows.
        build_query(db, tx, eval_incr_plan, sources)
    }

    fn eval_incr_query_expr<'a>(
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
                inserts.extend(Self::eval_query_expr_against_memtable(db, tx, &table.inserts, eval_incr_plan).iter());
            }
            if !table.deletes.is_empty() {
                deletes.extend(Self::eval_query_expr_against_memtable(db, tx, &table.deletes, eval_incr_plan).iter());
            }
        }

        UpdatesRelValue { deletes, inserts }
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
