// This file tests proposal 0022's `table = ...` modifier on column attributes:
// tables that share a row type get only the column attributes that name them, or that have no modifier.

use spacetimedb::table::TableInternal;
use spacetimedb::ReducerContext;

#[spacetimedb::table(accessor = with_pk)]
#[spacetimedb::table(accessor = without_pk)]
pub struct Row {
    #[primary_key(table = with_pk)]
    #[auto_inc(table = [with_pk])]
    id: u64,
    #[index(btree, table = without_pk)]
    name: String,
}

// The modifier still finds the struct's other tables when `table` is imported under another name.
use spacetimedb::table as tbl;

#[tbl(accessor = renamed_a)]
#[tbl(accessor = renamed_b)]
pub struct RenamedImportRow {
    #[primary_key(table = renamed_b)]
    id: u64,
}

#[spacetimedb::reducer]
fn use_tables(ctx: &ReducerContext) {
    let row = ctx.db.with_pk().id().find(1).unwrap();
    ctx.db.with_pk().id().update(row);
    for _ in ctx.db.without_pk().name().filter("name") {}
}

fn main() {
    // `register_table` declares these in the module def.
    assert_eq!(with_pk__TableHandle::PRIMARY_KEY, Some(0));
    assert_eq!(with_pk__TableHandle::UNIQUE_COLUMNS, [0]);
    assert_eq!(with_pk__TableHandle::SEQUENCES, [0]);
    let index_names = with_pk__TableHandle::INDEXES.iter().map(|index| index.source_name);
    assert_eq!(index_names.collect::<Vec<_>>(), ["with_pk_id_idx_btree"]);

    assert_eq!(without_pk__TableHandle::PRIMARY_KEY, None);
    assert!(without_pk__TableHandle::UNIQUE_COLUMNS.is_empty());
    assert!(without_pk__TableHandle::SEQUENCES.is_empty());
    let index_names = without_pk__TableHandle::INDEXES.iter().map(|index| index.source_name);
    assert_eq!(index_names.collect::<Vec<_>>(), ["without_pk_name_idx_btree"]);

    assert_eq!(renamed_a__TableHandle::PRIMARY_KEY, None);
    assert_eq!(renamed_b__TableHandle::PRIMARY_KEY, Some(0));
}
