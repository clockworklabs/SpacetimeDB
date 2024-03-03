use std::hash::Hash;

use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_primitives::TableId;

use crate::db::relational_db::{RelationalDB, Tx};
use crate::error::DBError;
use crate::execution_context::ExecutionContext;
use crate::host::module_host::{DatabaseTableUpdate, TableOp};

use super::query::{self, run_query, Kind, SupportedQuery};
use super::subscription::{eval_primary_updates, IncrementalJoin};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QueryHash {
    data: [u8; 32],
}

impl QueryHash {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            data: blake3::hash(bytes).into(),
        }
    }

    pub fn from_str(str: &str) -> Self {
        Self::from_bytes(str.as_bytes())
    }
}

#[derive(Debug)]
pub struct IncrementalUnit {
    hash: QueryHash,
    plan: SupportedQuery,
}

impl Eq for IncrementalUnit {}

impl PartialEq for IncrementalUnit {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl Hash for IncrementalUnit {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}

impl IncrementalUnit {
    pub fn new(plan: SupportedQuery, hash: QueryHash) -> Self {
        IncrementalUnit { hash, plan }
    }

    pub fn kind(&self) -> Kind {
        self.plan.kind
    }

    pub fn hash(&self) -> QueryHash {
        self.hash
    }

    pub fn return_table(&self) -> TableId {
        self.plan.return_table()
    }

    pub fn return_name(&self) -> String {
        self.plan.return_name()
    }

    pub fn filter_table(&self) -> TableId {
        self.plan.filter_table()
    }

    pub fn for_query(&self, hash: QueryHash) -> bool {
        self.hash == hash
    }

    #[tracing::instrument(skip_all)]
    pub fn eval(&self, db: &RelationalDB, tx: &Tx, auth: AuthCtx) -> Result<Option<DatabaseTableUpdate>, DBError> {
        let ctx = ExecutionContext::subscribe(db.address());
        let mut ops = vec![];
        for table in run_query(&ctx, db, tx, &self.plan.expr, auth)? {
            ops.extend(table.data.into_iter().map(TableOp::insert));
        }
        Ok((!ops.is_empty()).then(|| DatabaseTableUpdate {
            table_id: self.return_table(),
            table_name: self.return_name(),
            ops,
        }))
    }

    #[tracing::instrument(skip_all)]
    pub fn eval_incr<'a>(
        &'a self,
        db: &RelationalDB,
        tx: &Tx,
        tables: impl Iterator<Item = &'a DatabaseTableUpdate>,
        auth: AuthCtx,
    ) -> Result<Option<DatabaseTableUpdate>, DBError> {
        let ops = match self.plan.kind {
            Kind::Select => {
                let mut ops = Vec::new();
                for table in tables {
                    // Replace table reference in original query plan with virtual MemTable
                    let plan = query::to_mem_table(self.plan.expr.clone(), table);
                    // Evaluate the new plan and capture the new row operations.
                    ops.extend(eval_primary_updates(db, auth, tx, &plan)?.map(|r| TableOp::new(r.0, r.1)));
                }
                ops
            }
            Kind::Semijoin => {
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
