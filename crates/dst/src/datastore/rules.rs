use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::traits::IsolationLevel;
use spacetimedb_engine::error::{DBError, DatastoreError, IndexError};
use spacetimedb_lib::{AlgebraicValue, ProductValue};
use spacetimedb_sats::ArrayValue;

use super::world::{Datastore, OpenTx};
use super::{row_to_bytes, InsertOutcome, Row};
use crate::harness::{always_enabled, assert_always, assert_sometimes, Choices, Rule};
use crate::schema::{TablePlan, Type};

/// Weights reproduce the previous harness: outside a transaction, 99 begin
/// to 1 replay; inside one, 50 insert, 20 delete, 29 commit, 1 replay. The
/// previous harness committed when it picked delete with nothing deletable;
/// `commit_instead_of_delete` keeps that share, so transaction lengths match.
pub(super) const RULES: &[Rule<Datastore>] = &[
    Rule {
        name: "begin",
        weight: 99,
        when: not_in_tx,
        run: begin,
    },
    Rule {
        name: "insert",
        weight: 50,
        when: in_tx,
        run: insert,
    },
    Rule {
        name: "delete",
        weight: 20,
        when: in_tx_with_deletable_row,
        run: delete,
    },
    Rule {
        name: "commit_instead_of_delete",
        weight: 20,
        when: in_tx_without_deletable_row,
        run: commit,
    },
    Rule {
        name: "commit",
        weight: 29,
        when: in_tx,
        run: commit,
    },
    Rule {
        name: "replay",
        weight: 1,
        when: always_enabled,
        run: replay,
    },
];

fn in_tx(world: &Datastore) -> bool {
    world.tx.is_some()
}

fn not_in_tx(world: &Datastore) -> bool {
    world.tx.is_none()
}

fn in_tx_with_deletable_row(world: &Datastore) -> bool {
    deletable_table(world).is_some()
}

fn in_tx_without_deletable_row(world: &Datastore) -> bool {
    in_tx(world) && deletable_table(world).is_none()
}

fn begin(world: &mut Datastore, _: &mut Choices) -> anyhow::Result<()> {
    assert!(world.tx.is_none(), "guarded by `not_in_tx`");
    let tx = world
        .db()
        .begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
    let overlay = world.model.begin();
    world.tx = Some(OpenTx { tx, overlay });
    Ok(())
}

fn insert(world: &mut Datastore, choices: &mut Choices) -> anyhow::Result<()> {
    let table = insert_table(world, choices);
    let row = gen_insert_row(&world.schema.tables[table], choices);
    let table_id = world.table_ids[table];
    let (db, open, model) = world.tx_parts();

    let actual = match db.insert(&mut open.tx, table_id, &row_to_bytes(&row)) {
        Ok((_, inserted, _)) => InsertOutcome::Accepted(inserted.to_product_value()),
        // Generated rows can hit unique constraints on purpose; the model decides if that was right.
        Err(err) if is_unique_constraint_violation(&err) => InsertOutcome::UniqueConstraintViolation,
        Err(err) => return Err(err.into()),
    };
    // The model sees the row the datastore returned, so sequence values are real.
    let model_row = match &actual {
        InsertOutcome::Accepted(inserted) => inserted,
        InsertOutcome::UniqueConstraintViolation => &row,
    };
    let expected = model.insert(&mut open.overlay, table, model_row);

    assert_sometimes(
        actual == InsertOutcome::UniqueConstraintViolation,
        "insert hits a unique constraint",
    );
    assert_always(actual == expected, "insert matches model", || {
        format!("table {table}: datastore {actual:?}, model {expected:?}")
    })
}

fn delete(world: &mut Datastore, choices: &mut Choices) -> anyhow::Result<()> {
    let table = deletable_table(world).expect("guarded by `in_tx_with_deletable_row`");
    let table_id = world.table_ids[table];
    let (db, open, model) = world.tx_parts();

    let count = model.visible_count(&open.overlay, table) as u64;
    assert!(count > 0, "guarded by `in_tx_with_deletable_row`");
    // `below` returns less than `count`, a `usize` row count.
    let index = choices.below("delete.row", count) as usize;
    let row = model
        .visible_row(&open.overlay, table, index)
        .expect("`index` is below the visible row count")
        .clone();

    db.delete_by_rel(&mut open.tx, table_id, [row.clone()]);
    model.delete(&mut open.overlay, table, &row);
    Ok(())
}

fn commit(world: &mut Datastore, _: &mut Choices) -> anyhow::Result<()> {
    let open = world.tx.take().expect("guarded by `in_tx`");
    let Some((_tx_offset, tx_data, _tx_metrics, _reducer)) = world.db().commit_tx(open.tx)? else {
        anyhow::bail!("commit produced no transaction data");
    };
    let actual = world.commit_delta(&tx_data);
    let expected = world.model.commit(open.overlay);

    assert_sometimes(
        actual.tables.iter().any(|delta| delta.truncated),
        "commit truncates a table",
    );
    assert_always(actual == expected, "commit delta matches model", || {
        format!("datastore {actual:?}, model {expected:?}")
    })
}

fn replay(world: &mut Datastore, _: &mut Choices) -> anyhow::Result<()> {
    assert_sometimes(world.tx.is_some(), "replay with an open transaction");
    world.reopen_and_check()
}

/// The previous harness deleted only from the first table without a
/// sequence, so deletes never race the model on generated ids.
fn deletable_table(world: &Datastore) -> Option<usize> {
    let open = world.tx.as_ref()?;
    let auto_inc_table = world.schema.auto_inc_table_and_column().map(|(table, _)| table);
    let table = (0..world.schema.tables.len()).find(|&table| Some(table) != auto_inc_table)?;
    (world.model.visible_count(&open.overlay, table) > 0).then_some(table)
}

fn insert_table(world: &Datastore, choices: &mut Choices) -> usize {
    let auto_inc_table = world.schema.auto_inc_table_and_column().map(|(table, _)| table);
    match auto_inc_table {
        Some(table) if choices.below("insert.prefer_auto_inc", 3) != 0 => table,
        _ => choices.below("insert.table", world.schema.tables.len() as u64) as usize,
    }
}

fn gen_insert_row(table: &TablePlan, choices: &mut Choices) -> Row {
    let mut row: ProductValue = table
        .columns
        .iter()
        .map(|column| gen_value(column.ty, choices))
        .collect();

    // Zero asks the sequence for its next value.
    if let Some(sequence) = table.sequences.first() {
        row.elements[sequence.column] = match table.columns[sequence.column].ty {
            Type::I64 => AlgebraicValue::I64(0),
            Type::U64 => AlgebraicValue::U64(0),
            _ => unreachable!("`SequencePlan::new` only accepts integral columns"),
        };
    }
    row
}

fn gen_value(ty: Type, choices: &mut Choices) -> AlgebraicValue {
    match ty {
        Type::Bool => AlgebraicValue::Bool(choices.bool("value.bool")),
        Type::I64 => AlgebraicValue::I64(choices.u64("value.i64") as i64),
        Type::U64 => AlgebraicValue::U64(choices.u64("value.u64")),
        Type::String => AlgebraicValue::String(format!("v_{}", choices.u64("value.string")).into()),
        Type::Bytes => {
            let len = choices.below("value.bytes_len", 16);
            let bytes: Vec<u8> = (0..len).map(|_| choices.u64("value.byte") as u8).collect();
            AlgebraicValue::Array(ArrayValue::U8(bytes.into()))
        }
    }
}

fn is_unique_constraint_violation(error: &DBError) -> bool {
    matches!(
        error,
        DBError::Datastore(DatastoreError::Index(IndexError::UniqueConstraintViolation(_)))
    )
}
