use std::hash::Hash;

use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_primitives::TableId;
use spacetimedb_vm::expr::SourceSet;

use crate::db::relational_db::{RelationalDB, Tx};
use crate::error::DBError;
use crate::execution_context::ExecutionContext;
use crate::host::module_host::{DatabaseTableUpdate, TableOp};

use super::query::{self, run_query, Supported};
use super::subscription::{eval_primary_updates, IncrementalJoin, SupportedQuery};

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
    plan: SupportedQuery,
}

/// An ExecutionUnit is uniquely identified by its QueryHash.
impl Eq for ExecutionUnit {}

impl PartialEq for ExecutionUnit {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl From<SupportedQuery> for ExecutionUnit {
    fn from(plan: SupportedQuery) -> Self {
        ExecutionUnit {
            hash: QueryHash::NONE,
            plan,
        }
    }
}

impl ExecutionUnit {
    pub fn new(plan: SupportedQuery, hash: QueryHash) -> Self {
        ExecutionUnit { hash, plan }
    }

    /// Is this a single table select or a semijoin?
    pub fn kind(&self) -> Supported {
        self.plan.kind
    }

    /// The unique query hash for this execution unit.
    pub fn hash(&self) -> QueryHash {
        self.hash
    }

    /// The table from which this query returns rows.
    pub fn return_table(&self) -> TableId {
        self.plan.return_table()
    }

    pub fn return_name(&self) -> String {
        self.plan.return_name()
    }

    /// The table on which this query filters rows.
    /// In the case of a single table select,
    /// this is the same as the return table.
    /// In the case of a semijoin,
    /// it is the auxiliary table against which we are joining.
    pub fn filter_table(&self) -> TableId {
        self.plan.filter_table()
    }

    /// Evaluate this execution unit against the database.
    #[tracing::instrument(skip_all)]
    pub fn eval(&self, db: &RelationalDB, tx: &Tx, auth: AuthCtx) -> Result<Option<DatabaseTableUpdate>, DBError> {
        let ctx = ExecutionContext::subscribe(db.address());
        let mut ops = vec![];
        for table in run_query(&ctx, db, tx, &self.plan.expr, auth, SourceSet::default())? {
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
        let ops = match self.plan.kind {
            Supported::Select => {
                let mut ops = Vec::new();
                for table in tables.filter(|table| table.table_id == self.return_table()) {
                    // Replace table reference in original query plan with virtual MemTable
                    let (plan, sources) = query::to_mem_table(self.plan.expr.clone(), table);
                    // Evaluate the new plan and capture the new row operations.
                    ops.extend(eval_primary_updates(db, auth, tx, &plan, sources)?.map(|r| TableOp::new(r.0, r.1)));
                }
                ops
            }
            Supported::Semijoin => {
                if let Some(plan) = IncrementalJoin::new(&self.plan.expr, tables.into_iter())? {
                    // Evaluate the plan and capture the new row operations
                    plan.eval(db, tx, &auth)?.collect()
                } else {
                    vec![]
                }
            }
        };
        Ok((!ops.is_empty()).then(|| DatabaseTableUpdate {
            table_id: self.return_table(),
            table_name: self.return_name(),
            ops,
        }))
    }
}
