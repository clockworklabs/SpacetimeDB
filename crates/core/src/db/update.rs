use super::datastore::locking_tx_datastore::MutTxId;
use super::relational_db::RelationalDB;
use crate::database_logger::SystemLogger;
use crate::error::{DBError, TableError};
use anyhow::Context;
use core::{fmt, mem};
use enum_as_inner::EnumAsInner;
use itertools::Itertools;
use similar::{Algorithm, TextDiff};
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_lib::db::auth::StAccess;
use spacetimedb_lib::db::raw_def::{RawIndexDefV8, RawTableDefV8};
use spacetimedb_primitives::{ConstraintKind, Constraints, IndexId};
use spacetimedb_schema::schema::{ConstraintSchema, IndexSchema, SequenceSchema, TableSchema};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(thiserror::Error, Debug)]
pub enum UpdateDatabaseError {
    #[error("incompatible schema changes for: {tables:?}. See database log for details.")]
    IncompatibleSchema { tables: Vec<Box<str>> },
    #[error(transparent)]
    Database(#[from] DBError),
}

pub fn update_database(
    stdb: &RelationalDB,
    tx: &mut MutTxId,
    proposed_tables: Vec<RawTableDefV8>,
    system_logger: &SystemLogger,
) -> anyhow::Result<Result<(), UpdateDatabaseError>> {
    let existing_tables = stdb.get_all_tables_mut(tx)?;
    match schema_updates(existing_tables, proposed_tables)? {
        SchemaUpdates::Updated(updated) => {
            for (name, schema) in updated.new_tables {
                system_logger.info(&format!("Creating table `{}`", name));
                stdb.create_table(tx, schema)
                    .with_context(|| format!("failed to create table {}", name))?;
            }

            for (name, access) in updated.changed_access {
                stdb.alter_table_access(tx, name, access)?;
            }

            for (name, added) in updated.added_indexes {
                let table_id = stdb
                    .table_id_from_name_mut(tx, &name)?
                    .ok_or_else(|| TableError::NotFound(name.into()))?;
                for index in added {
                    stdb.create_index(tx, table_id, index)?;
                }
            }

            for index_id in updated.removed_indexes {
                stdb.drop_index(tx, index_id)?;
            }
        }

        SchemaUpdates::Tainted(tainted) => {
            system_logger.error("Module update rejected due to schema mismatch");
            let mut tables = Vec::with_capacity(tainted.len());
            for t in tainted {
                system_logger.warn(&format!("{}: {}", t.table_name, t.reason));
                if let TaintReason::IncompatibleSchema { existing, proposed } = t.reason {
                    let existing = format!("{existing:#?}");
                    let proposed = format!("{proposed:#?}");
                    let diff = TextDiff::configure()
                        .timeout(Duration::from_millis(200))
                        .algorithm(Algorithm::Patience)
                        .diff_lines(&existing, &proposed);
                    system_logger.warn(&format!(
                        "{}: Diff existing vs. proposed:\n{}",
                        t.table_name,
                        diff.unified_diff()
                    ));
                }
                tables.push(t.table_name);
            }
            return Ok(Err(UpdateDatabaseError::IncompatibleSchema { tables }));
        }
    }

    Ok(Ok(()))
}

/// The reasons a table can become [`Tainted`].
#[derive(Debug, Eq, PartialEq)]
pub enum TaintReason {
    /// The (row) schema changed, and we don't know how to go from A to B.
    IncompatibleSchema {
        existing: TableSchema,
        proposed: TableSchema,
    },
    /// The table is no longer present in the new schema.
    Orphaned,
}

impl fmt::Display for TaintReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::IncompatibleSchema { .. } => "incompatible schema",
            Self::Orphaned => "orphaned",
        })
    }
}

/// A table with name `table_name` marked tainted for reason [`TaintReason`].
#[derive(Debug, PartialEq)]
pub struct Tainted {
    pub table_name: Box<str>,
    pub reason: TaintReason,
}

#[derive(Debug, Default)]
pub struct Updates {
    /// Tables to create.
    new_tables: HashMap<Box<str>, RawTableDefV8>,
    /// The new table access levels.
    changed_access: HashMap<Box<str>, StAccess>,
    /// The indices added that are not a consequence of added constraints.
    added_indexes: HashMap<Box<str>, Vec<RawIndexDefV8>>,
    /// The indices removed that are not a consequence of removed constraints.
    removed_indexes: Vec<IndexId>,
}

#[derive(Debug, EnumAsInner)]
pub enum SchemaUpdates {
    /// The schema cannot be updated due to conflicts.
    Tainted(Vec<Tainted>),
    /// The schema can be updated.
    Updated(Updates),
}

/// Compute the diff between the current and proposed schema.
///
/// Compares all `existing_tables` loaded from the [`RelationalDB`] against the
/// proposed [`TableDef`]s. The proposed schemas are assumed to represent the
/// full schema information extracted from an STDB module.
///
/// Tables in the latter whose schema differs from the former are returned as
/// [`SchemaUpdates::Tainted`]. Tables also become tainted if they are
/// no longer present in the proposed schema (they are said to be "orphaned"),
/// although this restriction may be lifted in the future.
///
/// If no tables become tainted, the database may safely be updated using the
/// information in [`SchemaUpdates::Updates`].
pub fn schema_updates(
    existing_tables: impl IntoIterator<Item = Arc<TableSchema>>,
    proposed_tables: Vec<RawTableDefV8>,
) -> anyhow::Result<SchemaUpdates> {
    let mut updates = Updates::default();
    let mut tainted_tables = Vec::new();

    let mut known_tables: BTreeMap<Box<str>, Arc<TableSchema>> = existing_tables
        .into_iter()
        .map(|schema| (schema.table_name.clone(), schema))
        .collect();

    for proposed_schema_def in proposed_tables {
        let proposed_table_name = &proposed_schema_def.table_name;
        if let Some(known_schema) = known_tables.remove(proposed_table_name) {
            // Unfortunately `TableSchema::from_def . TableDef::from != id`.
            //
            // Namely, `from_def` inserts "generated" indexes, which are not
            // removed if we are roundtripping from an existing `TableSchema`.
            //
            // Also, there is no guarantee that the constituents of the schema
            // are sorted. They will be, however, when converting the proposed
            // `TableDef` into `TableSchema` (via `from_def`).
            let columns = known_schema
                .columns()
                .iter()
                .cloned()
                .sorted_by_key(|x| x.col_pos)
                .collect();
            let known_schema = TableSchema::new(
                known_schema.table_id,
                known_schema.table_name.clone(),
                columns,
                known_schema
                    .indexes
                    .iter()
                    .cloned()
                    .map(|x| IndexSchema {
                        index_id: if x.is_unique { 0.into() } else { x.index_id },
                        ..x
                    })
                    .sorted_by_key(|x| x.columns.clone())
                    .collect(),
                known_schema
                    .constraints
                    .iter()
                    .cloned()
                    .map(|x| ConstraintSchema {
                        constraint_id: 0.into(),
                        ..x
                    })
                    .filter(|x| x.constraints.kind() != ConstraintKind::UNSET)
                    .sorted_by_key(|x| x.columns.clone())
                    .collect(),
                known_schema
                    .sequences
                    .iter()
                    .cloned()
                    .map(|x| SequenceSchema {
                        sequence_id: 0.into(),
                        ..x
                    })
                    .sorted_by_key(|x| x.col_pos)
                    .collect(),
                known_schema.table_type,
                known_schema.table_access,
                known_schema.scheduled.clone(),
            );
            #[allow(deprecated)]
            let proposed_schema = TableSchema::from_def(known_schema.table_id, proposed_schema_def);

            let (schema_is_incompatible, known_schema, proposed_schema) =
                schema_compat_and_updates_hack(&mut updates, known_schema, proposed_schema);

            if schema_is_incompatible {
                log::warn!("Schema incompatible: {}", proposed_schema.table_name);
                log::debug!("Existing: {known_schema:?}");
                log::debug!("Proposed: {proposed_schema:?}");
                tainted_tables.push(Tainted {
                    table_name: proposed_schema.table_name.clone(),
                    reason: TaintReason::IncompatibleSchema {
                        existing: known_schema,
                        proposed: proposed_schema,
                    },
                });
            }
        } else {
            updates
                .new_tables
                .insert(proposed_table_name.to_owned(), proposed_schema_def);
        }
    }
    // We may at some point decide to drop orphaned tables automatically,
    // but for now it's an incompatible schema change
    for orphan in known_tables.into_keys() {
        if !orphan.starts_with("st_") {
            tainted_tables.push(Tainted {
                table_name: orphan,
                reason: TaintReason::Orphaned,
            });
        }
    }

    let res = if tainted_tables.is_empty() {
        SchemaUpdates::Updated(updates)
    } else {
        SchemaUpdates::Tainted(tainted_tables)
    };

    Ok(res)
}

/// Returns whether the `known` schema is compatible with the `prop`osed updated one.
///
/// Any compatible updates are extended into `updates`.
fn schema_compat_and_updates_hack(
    updates: &mut Updates,
    mut known: TableSchema,
    mut prop: TableSchema,
) -> (bool, TableSchema, TableSchema) {
    // HACK(Centril): Compute whether the new schema is incompatible with the old,
    // while ignoring `.table_access` and indices that aren't a consequence of a constraint.

    // Change the access of both known and proposed to `Public`.
    // We could have also picked `Private`.
    // It does not matter. We just want access to be irrelevant wrt. schema compatibility.
    let prop_access = mem::replace(&mut prop.table_access, StAccess::Public);
    let known_access = mem::replace(&mut known.table_access, StAccess::Public);
    // Record a change to the table access.
    let table_name = || known.table_name.clone();
    if prop_access != known_access {
        updates.changed_access.insert(table_name(), prop_access);
    }

    // If a proposed sequence has a lower allocation than an existing sequence,
    // i.e. the sequence in the DB has advanced beyond its initial allocation,
    // use the higher number.
    // Note that this still refuses updates where the proposed schema has a higher allocation
    // than the known schema,
    // since the actual update process will not alter the sequence.
    for (proposed_sequence, known_sequence) in prop.sequences.iter_mut().zip(known.sequences.iter()) {
        proposed_sequence.allocated = known_sequence.allocated.max(proposed_sequence.allocated);
    }

    // Filter out any indices that aren't a consequence of a constraint,
    // i.e., remove all non-generated indices.
    //
    // Fortunately for us, all generated indices satisfy `is_unique`,
    // as we have the following (from `ColumnAttribute`):
    //
    // UNSET = {}
    // INDEXED = { indexed }
    // AUTO_INC = { auto_inc }
    // UNIQUE = INDEXED | { unique } = { unique, indexed }
    // IDENTITY = UNIQUE | AUTO_INC = { unique, indexed, auto_inc }
    // PRIMARY_KEY = UNIQUE | { pk } = { unique, indexed, pk }
    // PRIMARY_KEY_AUTO = PRIMARY_KEY | AUTO_INC = { unique, indexed, auto_inc, pk }
    // PRIMARY_KEY_IDENTITY = PRIMARY_KEY | IDENTITY =  = { unique, indexed, auto_inc, pk }
    //
    // This entails that all attributes with `indexed`,
    // that have something additional,
    // also have `unique`.
    let prop_indexes = prop.indexes.clone();
    let known_indexes = known.indexes.clone();

    // Separate the generated and non-generated proposed and known indices.
    let (prop_gen_indexes, mut prop_spec_indexes) = prop.indexes.into_iter().partition(|idx| idx.is_unique);
    let (known_gen_indexes, mut known_spec_indexes) = known.indexes.into_iter().partition(|idx| idx.is_unique);
    prop.indexes = prop_gen_indexes;
    known.indexes = known_gen_indexes;

    // These indices are not in `known_spec_indexes`, so they were added.
    prop_spec_indexes.retain(|pidx| !known_spec_indexes.iter().any(|kidx| kidx.index_name == pidx.index_name));
    if !prop_spec_indexes.is_empty() {
        updates
            .added_indexes
            .insert(table_name(), prop_spec_indexes.into_iter().map_into().collect());
    }

    // These indices are not in `proposed_indexes`, so they were removed.
    known_spec_indexes.retain(|kidx| !prop_indexes.iter().any(|pidx| kidx.index_name == pidx.index_name));
    updates
        .removed_indexes
        .extend(known_spec_indexes.into_iter().map(|idx| idx.index_id));

    // Strip constraints that are just `Indexed`.
    let prop_constraints = prop.constraints.clone();
    let known_constraints = known.constraints.clone();
    prop.constraints.retain(|c| c.constraints != Constraints::indexed());
    known.constraints.retain(|c| c.constraints != Constraints::indexed());

    // Now, while ignoring `table_access` and indices,
    // and permitting sequence allocation advances,
    // compute schema compatibility.
    let changed = prop != known;

    // Revert back to the original proposed schema.
    prop.table_access = prop_access;
    prop.indexes = prop_indexes;
    prop.constraints = prop_constraints;
    // Revert back to the original known schema.
    known.table_access = known_access;
    known.indexes = known_indexes;
    known.constraints = known_constraints;

    // Do *not* revert sequence allocations; we intend to use the altered sequence allocation value.

    (changed, known, prop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use spacetimedb_lib::db::auth::{StAccess, StTableType};
    use spacetimedb_lib::db::raw_def::{IndexType, RawColumnDefV8};
    use spacetimedb_primitives::{col_list, ColId, Constraints, IndexId, TableId};
    use spacetimedb_sats::AlgebraicType;
    use spacetimedb_schema::schema::{ColumnSchema, IndexSchema};

    #[test]
    fn test_updates_new_table() -> anyhow::Result<()> {
        let table_id = TableId(42);
        let table_name = "Person";
        let index_id = IndexId(24);
        let current = [Arc::new(TableSchema::new(
            table_id,
            table_name.into(),
            vec![ColumnSchema {
                table_id,
                col_pos: ColId(0),
                col_name: "name".into(),
                col_type: AlgebraicType::String,
            }],
            vec![IndexSchema {
                table_id,
                index_id,
                index_type: IndexType::BTree,
                index_name: "known_index".into(),
                is_unique: false,
                columns: col_list![0],
            }],
            vec![],
            vec![],
            StTableType::User,
            StAccess::Public,
            None,
        ))];
        let proposed_indexes = vec![RawIndexDefV8 {
            index_type: IndexType::BTree,
            index_name: "proposed_index".into(),
            is_unique: false,
            columns: col_list![0],
        }];
        let proposed = vec![
            RawTableDefV8::new(
                table_name.into(),
                vec![RawColumnDefV8 {
                    col_name: "name".into(),
                    col_type: AlgebraicType::String,
                }],
            )
            .with_access(StAccess::Private)
            .with_indexes(proposed_indexes.clone()),
            RawTableDefV8::new(
                "Pet".into(),
                vec![RawColumnDefV8 {
                    col_name: "furry".into(),
                    col_type: AlgebraicType::Bool,
                }],
            ),
        ];

        let updates = schema_updates(current, proposed.clone())?
            .into_updated()
            .map_err(|su| anyhow!("unexpectedly tainted: {su:#?}"))?;
        assert_eq!(updates.new_tables.len(), 1);
        assert_eq!(updates.new_tables.get("Pet"), proposed.last());

        assert_eq!(
            updates.changed_access.into_iter().collect::<Vec<_>>(),
            [(table_name.into(), StAccess::Private)]
        );

        assert_eq!(updates.removed_indexes, [index_id]);
        let mut added_indexes2 = HashMap::new();
        added_indexes2.insert(table_name.into(), proposed_indexes);
        assert_eq!(updates.added_indexes, added_indexes2);

        Ok(())
    }

    #[test]
    fn test_updates_schema_mismatch() {
        #[allow(deprecated)]
        let current = [Arc::new(TableSchema::from_def(
            42.into(),
            RawTableDefV8::new(
                "Person".into(),
                vec![RawColumnDefV8 {
                    col_name: "name".into(),
                    col_type: AlgebraicType::String,
                }],
            ),
        ))];
        let proposed = vec![RawTableDefV8::new(
            "Person".into(),
            vec![
                RawColumnDefV8 {
                    col_name: "id".into(),
                    col_type: AlgebraicType::U32,
                },
                RawColumnDefV8 {
                    col_name: "name".into(),
                    col_type: AlgebraicType::String,
                },
            ],
        )
        .with_column_constraint(Constraints::identity(), ColId(0))];

        assert_incompatible_schema(schema_updates(current, proposed).unwrap(), &["Person"]);
    }

    #[test]
    fn test_updates_orphaned_table() {
        #[allow(deprecated)]
        let current = [Arc::new(TableSchema::from_def(
            42.into(),
            RawTableDefV8::new(
                "Person".into(),
                vec![RawColumnDefV8 {
                    col_name: "name".into(),
                    col_type: AlgebraicType::String,
                }],
            ),
        ))];
        let proposed = vec![RawTableDefV8::new(
            "Pet".into(),
            vec![RawColumnDefV8 {
                col_name: "furry".into(),
                col_type: AlgebraicType::Bool,
            }],
        )];

        assert_orphaned(schema_updates(current, proposed).unwrap(), &["Person"]);
    }

    #[test]
    fn test_updates_add_index() {
        let table_def = RawTableDefV8::new(
            "Person".into(),
            vec![RawColumnDefV8 {
                col_name: "name".into(),
                col_type: AlgebraicType::String,
            }],
        );
        #[allow(deprecated)]
        let current = [Arc::new(TableSchema::from_def(42.into(), table_def.clone()))];
        let proposed = vec![table_def.with_column_index(ColId(0), false)];

        let updates = schema_updates(current, proposed).unwrap().into_updated().unwrap();
        assert_eq!(updates.added_indexes["Person"].len(), 1);
        assert_eq!(updates.removed_indexes, []);
    }

    #[test]
    fn test_updates_drop_index() {
        let table_id = TableId(42);
        let index_id = IndexId(68);
        let current = [Arc::new(TableSchema::new(
            table_id,
            "Person".into(),
            vec![ColumnSchema {
                table_id,
                col_pos: ColId(0),
                col_name: "name".into(),
                col_type: AlgebraicType::String,
            }],
            vec![IndexSchema {
                index_id,
                table_id,
                index_type: IndexType::BTree,
                index_name: "bobson_dugnutt".into(),
                is_unique: false,
                columns: ColId(0).into(),
            }],
            vec![],
            vec![],
            StTableType::User,
            StAccess::Public,
            None,
        ))];
        let proposed = vec![RawTableDefV8::new(
            "Person".into(),
            vec![RawColumnDefV8 {
                col_name: "name".into(),
                col_type: AlgebraicType::String,
            }],
        )];

        let updates = schema_updates(current, proposed).unwrap().into_updated().unwrap();
        assert_eq!(updates.added_indexes.len(), 0);
        assert_eq!(updates.removed_indexes, [index_id]);
    }

    #[test]
    fn test_updates_add_constraint() {
        #[allow(deprecated)]
        let current = [Arc::new(TableSchema::from_def(
            42.into(),
            RawTableDefV8::new(
                "Person".into(),
                vec![RawColumnDefV8 {
                    col_name: "name".into(),
                    col_type: AlgebraicType::String,
                }],
            ),
        ))];
        let proposed = vec![RawTableDefV8::new(
            "Person".into(),
            vec![RawColumnDefV8 {
                col_name: "name".into(),
                col_type: AlgebraicType::String,
            }],
        )
        .with_column_constraint(Constraints::unique(), ColId(0))];

        assert_incompatible_schema(schema_updates(current, proposed).unwrap(), &["Person"]);
    }

    #[test]
    fn test_updates_drop_constraint() {
        #[allow(deprecated)]
        let current = [Arc::new(TableSchema::from_def(
            42.into(),
            RawTableDefV8::new(
                "Person".into(),
                vec![RawColumnDefV8 {
                    col_name: "name".into(),
                    col_type: AlgebraicType::String,
                }],
            )
            .with_column_constraint(Constraints::unique(), ColId(0)),
        ))];
        let proposed = vec![RawTableDefV8::new(
            "Person".into(),
            vec![RawColumnDefV8 {
                col_name: "name".into(),
                col_type: AlgebraicType::String,
            }],
        )];

        assert_incompatible_schema(schema_updates(current, proposed).unwrap(), &["Person"]);
    }

    fn assert_incompatible_schema(result: SchemaUpdates, tainted_tables: &[&str]) {
        assert_tainted(result, tainted_tables, |reason| {
            matches!(reason, TaintReason::IncompatibleSchema { .. })
        });
    }

    fn assert_orphaned(result: SchemaUpdates, tainted_tables: &[&str]) {
        assert_tainted(result, tainted_tables, |reason| matches!(reason, TaintReason::Orphaned))
    }

    fn assert_tainted(result: SchemaUpdates, tainted_tables: &[&str], match_reason: impl Fn(&TaintReason) -> bool) {
        let tainted = result.into_tainted().unwrap();
        let mut actual_tainted_tables = Vec::with_capacity(tainted.len());
        for t in tainted {
            assert!(
                match_reason(&t.reason),
                "{}: unexpected taint reason: {:#?}",
                t.table_name,
                t.reason
            );
            actual_tainted_tables.push(t.table_name.to_string());
        }
        assert_eq!(&actual_tainted_tables, tainted_tables);
    }
}
