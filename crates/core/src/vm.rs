use crate::db::relational_db::{RelationalDB, Tx};
use crate::error::DBError;
use spacetimedb_lib::identity::AuthCtx;

/// If the caller is not allowed to exceed the row limit,
/// reject the request if the estimated cardinality exceeds the limit.
pub fn check_row_limit<Query>(
    queries: &[Query],
    db: &RelationalDB,
    tx: &Tx,
    row_est: impl Fn(&Query, &Tx) -> u64,
    auth: &AuthCtx,
) -> Result<(), DBError> {
    if !auth.exceed_row_limit()
        && let Some(limit) = db.row_limit(tx)?
    {
        let mut estimate: u64 = 0;
        for query in queries {
            estimate = estimate.saturating_add(row_est(query, tx));
        }
        if estimate > limit {
            return Err(DBError::Other(anyhow::anyhow!(
                "Estimated cardinality ({estimate} rows) exceeds limit ({limit} rows)"
            )));
        }
    }
    Ok(())
}
