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

    let mut inserts = vec![];
    let mut deletes = vec![];

    let mut duplicate_rows_evaluated = 0;
    let mut duplicate_rows_sent = 0;

    // Query plans for joins may return redundant rows,
    // but we track row counts to avoid sending them to clients.
    //
    // Single table plans will never return redundant rows,
    // so there's no need to track row counts.
    if !plan.is_join() {
        plan.for_each_insert(tx, metrics, &mut |row| {
            inserts.push(row.into());
            Ok(())
        })?;

        plan.for_each_delete(tx, metrics, &mut |row| {
            deletes.push(row.into());
            Ok(())
        })?;
    } else {
        let mut insert_counts = HashMap::new();
        let mut delete_counts = HashMap::new();

        plan.for_each_insert(tx, metrics, &mut |row| {
            let n = insert_counts.entry(row).or_default();
            if *n > 0 {
                duplicate_rows_evaluated += 1;
            }
            *n += 1;
            Ok(())
        })?;

        plan.for_each_delete(tx, metrics, &mut |row| {
            match insert_counts.get_mut(&row) {
                // We have not seen an insert for this row.
                // If we have seen a delete, increment the metric.
                // Always increment the delete_count.
                None => {
                    let n = delete_counts.entry(row).or_default();
                    if *n > 0 {
                        duplicate_rows_evaluated += 1;
                    }
                    *n += 1;
                }
                // We have already seen an insert for this row.
                // This is a duplicate, so increment the metric.
                //
                // There are no more inserts for this row,
                // so increment the delete_count as well.
                Some(0) => {
                    duplicate_rows_evaluated += 1;
                    *delete_counts.entry(row).or_default() += 1;
                }
                // We have already seen an insert for this row.
                // This is a duplicate, so increment the metric.
                //
                // There are still more inserts for this row,
                // so don't increment the delete_count.
                Some(n) => {
                    duplicate_rows_evaluated += 1;
                    *n -= 1;
                }
            }
            Ok(())
        })?;

        for (row, n) in insert_counts.into_iter().filter(|(_, n)| *n > 0) {
            duplicate_rows_sent += n as u64 - 1;
            inserts.extend(std::iter::repeat_n(row, n).map(RelValue::from));
        }
        for (row, n) in delete_counts.into_iter().filter(|(_, n)| *n > 0) {
            duplicate_rows_sent += n as u64 - 1;
            deletes.extend(std::iter::repeat_n(row, n).map(RelValue::from));
        }
    }

    // Return `None` for empty updates
    if inserts.is_empty() && deletes.is_empty() {
        return Ok(None);
    }

    metrics.delta_queries_matched += 1;
    metrics.duplicate_rows_evaluated += duplicate_rows_evaluated;
    metrics.duplicate_rows_sent += duplicate_rows_sent;

    Ok(Some(UpdatesRelValue { inserts, deletes }))
}
