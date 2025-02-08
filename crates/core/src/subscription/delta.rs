use std::collections::HashMap;

use anyhow::Result;
use spacetimedb_execution::{Datastore, DeltaStore};
use spacetimedb_lib::metrics::ExecutionMetrics;
use spacetimedb_subscription::SubscriptionPlan;
use spacetimedb_vm::relation::RelValue;

use crate::host::module_host::UpdatesRelValue;

/// This utility deduplicates an incremental update.
/// That is, if a row is both inserted and deleted,
/// this method removes it from the result set.
///
/// Note, the 1.0 api does allow for duplicate rows.
/// Hence this may be removed at any time after 1.0.
pub fn eval_delta<'a, Tx: Datastore + DeltaStore>(
    tx: &'a Tx,
    metrics: &mut ExecutionMetrics,
    plan: &SubscriptionPlan,
) -> Result<UpdatesRelValue<'a>> {
    // Note, we can't determine apriori what capacity to allocate
    let mut inserts = HashMap::new();
    let mut deletes = vec![];

    plan.for_each_insert(tx, metrics, &mut |row| {
        inserts
            .entry(RelValue::from(row))
            // Row already inserted?
            // Increment its multiplicity.
            .and_modify(|n| *n += 1)
            .or_insert(1);
        Ok(())
    })?;

    plan.for_each_delete(tx, metrics, &mut |row| {
        let row = RelValue::from(row);
        match inserts.get_mut(&row) {
            // This row was not inserted.
            // Add it to the delete set.
            None => {
                deletes.push(row);
            }
            // This row was inserted.
            // Decrement the multiplicity.
            Some(1) => {
                inserts.remove(&row);
            }
            // This row was inserted.
            // Decrement the multiplicity.
            Some(n) => {
                *n -= 1;
            }
        }
        Ok(())
    })?;

    Ok(UpdatesRelValue {
        inserts: inserts.into_keys().collect(),
        deletes,
    })
}
