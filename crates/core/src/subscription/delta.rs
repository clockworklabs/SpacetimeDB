use anyhow::Result;
use hashbrown::HashMap;
use spacetimedb_execution::{Datastore, DeltaStore};
use spacetimedb_lib::metrics::ExecutionMetrics;
use spacetimedb_subscription::SubscriptionPlan;
use spacetimedb_vm::relation::RelValue;

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

    let mut insert_counts = HashMap::new();
    let mut delete_counts = HashMap::new();

    plan.for_each_insert(tx, metrics, &mut |row| {
        *insert_counts.entry(row).or_default() += 1;
        Ok(())
    })?;

    plan.for_each_delete(tx, metrics, &mut |row| {
        match insert_counts.get_mut(&row) {
            None | Some(0) => {
                *delete_counts.entry(row).or_default() += 1;
            }
            Some(n) => {
                *n -= 1;
            }
        }
        Ok(())
    })?;

    let mut inserts = vec![];
    let mut deletes = vec![];

    for (row, n) in insert_counts.into_iter().filter(|(_, n)| *n > 0) {
        inserts.extend(std::iter::repeat_n(row, n).map(RelValue::from));
    }
    for (row, n) in delete_counts.into_iter().filter(|(_, n)| *n > 0) {
        deletes.extend(std::iter::repeat_n(row, n).map(RelValue::from));
    }

    // Return `None` for empty updates
    if inserts.is_empty() && deletes.is_empty() {
        return Ok(None);
    }

    metrics.delta_queries_matched += 1;
    Ok(Some(UpdatesRelValue { inserts, deletes }))
}
