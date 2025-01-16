use std::collections::HashMap;

use anyhow::Result;
use spacetimedb_execution::{Datastore, DeltaStore};
use spacetimedb_query::delta::DeltaPlanEvaluator;
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
    delta: &'a DeltaPlanEvaluator,
) -> Result<UpdatesRelValue<'a>> {
    if !delta.is_join() {
        return Ok(UpdatesRelValue {
            inserts: delta.eval_inserts(tx)?.map(RelValue::from).collect(),
            deletes: delta.eval_deletes(tx)?.map(RelValue::from).collect(),
        });
    }
    if delta.has_inserts() && !delta.has_deletes() {
        return Ok(UpdatesRelValue {
            inserts: delta.eval_inserts(tx)?.map(RelValue::from).collect(),
            deletes: vec![],
        });
    }
    if delta.has_deletes() && !delta.has_inserts() {
        return Ok(UpdatesRelValue {
            deletes: delta.eval_deletes(tx)?.map(RelValue::from).collect(),
            inserts: vec![],
        });
    }
    let mut inserts = HashMap::new();

    for row in delta.eval_inserts(tx)?.map(RelValue::from) {
        inserts.entry(row).and_modify(|n| *n += 1).or_insert(1);
    }

    let deletes = delta
        .eval_deletes(tx)?
        .map(RelValue::from)
        .filter(|row| match inserts.get_mut(row) {
            None => true,
            Some(1) => inserts.remove(row).is_none(),
            Some(n) => {
                *n -= 1;
                false
            }
        })
        .collect();

    Ok(UpdatesRelValue {
        inserts: inserts.into_keys().collect(),
        deletes,
    })
}
