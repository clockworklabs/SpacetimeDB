use crate::host::module_host::UpdatesRelValue;
use anyhow::Result;
use spacetimedb_data_structures::map::{HashCollectionExt as _, HashMap};
use spacetimedb_execution::{bindings::BindingId, Datastore, DeltaStore, RelValue, Row};
use spacetimedb_lib::metrics::ExecutionMetrics;
use spacetimedb_primitives::ColList;
use spacetimedb_sats::product_value::InvalidFieldError;
use spacetimedb_subscription::TemplateFragment;
use std::collections::BTreeMap;
/// Evaluate a template over a delta update, grouping the rows by the binding they matched.
/// Leaves out the bindings without updates.
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
    plan: &TemplateFragment,
) -> Result<BTreeMap<BindingId, UpdatesRelValue<'a>>> {
    metrics.delta_queries_evaluated += 1;

    let mut inserts = BTreeMap::<BindingId, Vec<RelValue<'a>>>::new();
    let mut deletes = BTreeMap::<BindingId, Vec<RelValue<'a>>>::new();

    let mut duplicate_rows_evaluated = 0;
    let mut duplicate_rows_sent = 0;

    let col_list = ColList::from_iter(plan.num_private_cols()..plan.num_cols());

    let maybe_project = |row: Row<'a>| -> Result<RelValue<'a>, InvalidFieldError> {
        if plan.is_view() {
            Ok(row.project_product(&col_list)?.into())
        } else {
            Ok(row.into())
        }
    };

    if !plan.is_join() {
        // Single table plans will never return redundant rows,
        // and a lookup pairs a row with each binding once,
        // so there's no need to track row counts.
        plan.for_each_insert(tx, metrics, &mut |id, row| {
            inserts.entry(id).or_default().push(maybe_project(row)?);
            Ok(())
        })?;

        plan.for_each_delete(tx, metrics, &mut |id, row| {
            deletes.entry(id).or_default().push(maybe_project(row)?);
            Ok(())
        })?;
    } else {
        // Query plans for joins may return redundant rows.
        // We track row counts, per binding, to avoid sending them to clients.
        let mut insert_counts = HashMap::new();
        let mut delete_counts = HashMap::new();

        plan.for_each_insert(tx, metrics, &mut |id, row| {
            let row = (id, maybe_project(row)?);
            let n = insert_counts.entry(row).or_default();
            if *n > 0 {
                duplicate_rows_evaluated += 1;
            }
            *n += 1;
            Ok(())
        })?;

        plan.for_each_delete(tx, metrics, &mut |id, row| {
            let row = (id, maybe_project(row)?);
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

        for ((id, row), n) in insert_counts.into_iter().filter(|(_, n)| *n > 0) {
            duplicate_rows_sent += n as u64 - 1;
            inserts.entry(id).or_default().extend(std::iter::repeat_n(row, n));
        }
        for ((id, row), n) in delete_counts.into_iter().filter(|(_, n)| *n > 0) {
            duplicate_rows_sent += n as u64 - 1;
            deletes.entry(id).or_default().extend(std::iter::repeat_n(row, n));
        }
    }

    // Return no bindings for empty updates
    if inserts.is_empty() && deletes.is_empty() {
        return Ok(BTreeMap::new());
    }

    metrics.delta_queries_matched += 1;
    metrics.duplicate_rows_evaluated += duplicate_rows_evaluated;
    metrics.duplicate_rows_sent += duplicate_rows_sent;

    // Pair each binding's inserts and deletes
    let mut updates = BTreeMap::<BindingId, UpdatesRelValue<'a>>::new();
    for (id, rows) in inserts {
        updates.entry(id).or_default().inserts = rows;
    }
    for (id, rows) in deletes {
        updates.entry(id).or_default().deletes = rows;
    }
    Ok(updates)
}
