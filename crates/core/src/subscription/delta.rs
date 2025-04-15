use anyhow::Result;
use spacetimedb_execution::{Datastore, DeltaStore};
use spacetimedb_lib::metrics::ExecutionMetrics;
use spacetimedb_subscription::SubscriptionPlan;

use crate::host::module_host::UpdatesRelValue;

/// Evaluate a subscription over a delta update.
/// Returns `None` for empty updates.
///
/// IMPORTANT: This does and must implement bag semantics.
/// That is, we must not remove duplicate rows.
/// Any deviation from this is a bug, as clients will lose information.
///
/// Take for example the semijoin R ⋉ S.
/// A client needs to know for each row in R,
/// how many rows it joins with in S.
pub fn eval_delta<'a, Tx: Datastore + DeltaStore>(
    tx: &'a Tx,
    metrics: &mut ExecutionMetrics,
    plan: &SubscriptionPlan,
) -> Result<Option<UpdatesRelValue<'a>>> {
    metrics.delta_queries_evaluated += 1;
    let mut inserts = vec![];
    let mut deletes = vec![];

    plan.for_each_insert(tx, metrics, &mut |row| {
        inserts.push(row.into());
        Ok(())
    })?;

    plan.for_each_delete(tx, metrics, &mut |row| {
        deletes.push(row.into());
        Ok(())
    })?;

    // Return `None` for empty updates
    if inserts.is_empty() && deletes.is_empty() {
        return Ok(None);
    }

    metrics.delta_queries_matched += 1;
    Ok(Some(UpdatesRelValue { inserts, deletes }))
}
