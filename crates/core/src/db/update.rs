use core::fmt;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};

use anyhow::Context;
use spacetimedb_lib::Hash;

use crate::database_logger::SystemLogger;
use crate::error::DBError;
use crate::execution_context::ExecutionContext;

use super::datastore::locking_tx_datastore::MutTxId;
use super::datastore::traits::{IndexDef, TableDef, TableSchema};
use super::relational_db::RelationalDB;
use spacetimedb_primitives::IndexId;

#[derive(thiserror::Error, Debug)]
pub enum UpdateDatabaseError {
    #[error("incompatible schema changes for: {tables:?}")]
    IncompatibleSchema { tables: Vec<String> },
    #[error(transparent)]
    Database(#[from] DBError),
}

// TODO: Post #267, it will no longer be possible to modify indexes on existing
// tables. Below must thus be simplified to reject _any_ change to existing
// tables, and only accept updates which introduce new tables (beware of
// ordering when comparing definition types!).

pub fn update_database(
    stdb: &RelationalDB,
    tx: MutTxId,
    proposed_tables: Vec<TableDef>,
    fence: u128,
    module_hash: Hash,
    system_logger: &SystemLogger,
) -> anyhow::Result<Result<MutTxId, UpdateDatabaseError>> {
    let ctx = ExecutionContext::internal(stdb.address());
    let (tx, res) = stdb.with_auto_rollback::<_, _, anyhow::Error>(&ctx, tx, |tx| {
        let existing_tables = stdb.get_all_tables(tx)?;
        match schema_updates(existing_tables, proposed_tables)? {
            SchemaUpdates::Updates {
                new_tables,
                indexes_to_drop,
                indexes_to_create,
            } => {
                for (name, schema) in new_tables {
                    system_logger.info(&format!("Creating table `{}`", name));
                    stdb.create_table(tx, schema)
                        .with_context(|| format!("failed to create table {}", name))?;
                }

                for index_id in indexes_to_drop {
                    system_logger.info(&format!("Dropping index with id {}", index_id.0));
                    stdb.drop_index(tx, index_id)?;
                }

                for index_def in indexes_to_create {
                    system_logger.info(&format!("Creating index `{}`", index_def.name));
                    stdb.create_index(tx, index_def)?;
                }
            }

            SchemaUpdates::Tainted(tainted) => {
                system_logger.error("Module update rejected due to schema mismatch");
                let mut tables = Vec::with_capacity(tainted.len());
                for t in tainted {
                    system_logger.warn(&format!("{}: {}", t.table_name, t.reason));
                    tables.push(t.table_name);
                }
                return Ok(Err(UpdateDatabaseError::IncompatibleSchema { tables }));
            }
        }

        // Update the module hash. Morally, this should be done _after_ calling
        // the `update` reducer, but that consumes our transaction context.
        stdb.set_program_hash(tx, fence, module_hash)?;

        Ok(Ok(()))
    })?;
    Ok(stdb.rollback_on_err(&ctx, tx, res).map(|(tx, ())| tx))
}

/// The reasons a table can become [`Tainted`].
#[derive(Debug, Eq, PartialEq)]
pub enum TaintReason {
    /// The (row) schema changed, and we don't know how to go from A to B.
    IncompatibleSchema,
    /// The table is no longer present in the new schema.
    Orphaned,
}

impl fmt::Display for TaintReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::IncompatibleSchema => "incompatible schema",
            Self::Orphaned => "orphaned",
        })
    }
}

/// A table with name `table_name` marked tainted for reason [`TaintReason`].
#[derive(Debug, PartialEq)]
pub struct Tainted {
    pub table_name: String,
    pub reason: TaintReason,
}

#[derive(Debug)]
pub enum SchemaUpdates {
    /// The schema cannot be updated due to conflicts.
    Tainted(Vec<Tainted>),
    /// The schema can be updates.
    Updates {
        /// Tables to create.
        new_tables: HashMap<String, TableDef>,
        /// Indexes to drop.
        ///
        /// Should be processed _before_ `indexes_to_create`, as we might be
        /// updating (i.e. drop then create with different parameters).
        indexes_to_drop: Vec<IndexId>,
        /// Indexes to create.
        ///
        /// Should be processed _after_ `indexes_to_drop`.
        indexes_to_create: Vec<IndexDef>,
    },
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
    existing_tables: Vec<Cow<'_, TableSchema>>,
    proposed_tables: Vec<TableDef>,
) -> anyhow::Result<SchemaUpdates> {
    let mut new_tables = HashMap::new();
    let mut tainted_tables = Vec::new();
    let mut indexes_to_create = Vec::new();
    let mut indexes_to_drop = Vec::new();

    let mut known_tables: BTreeMap<String, Cow<TableSchema>> = existing_tables
        .into_iter()
        .map(|schema| (schema.table_name.clone(), schema))
        .collect();

    for proposed_schema_def in proposed_tables {
        let proposed_table_name = &proposed_schema_def.table_name;
        if let Some(known_schema) = known_tables.remove(proposed_table_name) {
            let table_id = known_schema.table_id;
            let known_schema_def = TableDef::from(known_schema.as_ref());
            // If the schemas differ the update should be rejected.
            if !equiv(&known_schema_def, &proposed_schema_def) {
                tainted_tables.push(Tainted {
                    table_name: proposed_table_name.to_owned(),
                    reason: TaintReason::IncompatibleSchema,
                });
            } else {
                // The schema is unchanged, but maybe the indexes are.
                let mut known_indexes = known_schema
                    .indexes
                    .iter()
                    .map(|idx| (idx.index_name.clone(), idx))
                    .collect::<BTreeMap<_, _>>();

                for mut index_def in proposed_schema_def.indexes {
                    // This is zero in the proposed schema, as the table id
                    // is not known at proposal time.
                    index_def.table_id = table_id;

                    match known_indexes.remove(&index_def.name) {
                        None => indexes_to_create.push(index_def),
                        Some(known_index) => {
                            let known_id = known_index.index_id;
                            let known_index_def = IndexDef::from(known_index.clone());
                            if known_index_def != index_def {
                                indexes_to_drop.push(known_id);
                                indexes_to_create.push(index_def);
                            }
                        }
                    }
                }

                // Indexes not in the proposed schema shall be dropped.
                for index in known_indexes.into_values() {
                    indexes_to_drop.push(index.index_id);
                }
            }
        } else {
            new_tables.insert(proposed_table_name.to_owned(), proposed_schema_def);
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
        SchemaUpdates::Updates {
            new_tables,
            indexes_to_drop,
            indexes_to_create,
        }
    } else {
        SchemaUpdates::Tainted(tainted_tables)
    };

    Ok(res)
}

/// Two [`datastore::traits::TableDef`]s are equivalent if, and only if, all
/// their fields _except_ for `indexes` are equal.
///
/// This allows to reject schema changes in [`schema_updates`] but allow
/// changes to only the indexes. We don't have support for full schema
/// migrations yet, but creating and dropping indexes is trivial.
fn equiv(a: &TableDef, b: &TableDef) -> bool {
    let TableDef {
        table_name,
        columns,
        indexes: _,
        table_type,
        table_access,
    } = a;
    table_name == &b.table_name
        && table_type == &b.table_type
        && table_access == &b.table_access
        && columns == &b.columns
}

#[cfg(test)]
mod tests {
    use anyhow::bail;
    use nonempty::NonEmpty;
    use spacetimedb_lib::{
        auth::{StAccess, StTableType},
        IndexType,
    };
    use spacetimedb_primitives::{ColId, TableId};
    use spacetimedb_sats::AlgebraicType;

    use crate::db::datastore::traits::{ColumnDef, ColumnSchema, IndexSchema, AUTO_TABLE_ID};

    use super::*;

    #[test]
    fn test_updates_new_table() -> anyhow::Result<()> {
        let current = vec![Cow::Owned(TableSchema {
            table_id: TableId(42),
            table_name: "Person".into(),
            columns: vec![ColumnSchema {
                table_id: TableId(42),
                col_id: ColId(0),
                col_name: "name".into(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            }],
            indexes: vec![],
            constraints: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        })];
        let proposed = vec![
            TableDef {
                table_name: "Person".into(),
                columns: vec![ColumnDef {
                    col_name: "name".into(),
                    col_type: AlgebraicType::String,
                    is_autoinc: false,
                }],
                indexes: vec![],
                table_type: StTableType::User,
                table_access: StAccess::Public,
            },
            TableDef {
                table_name: "Pet".into(),
                columns: vec![ColumnDef {
                    col_name: "furry".into(),
                    col_type: AlgebraicType::Bool,
                    is_autoinc: false,
                }],
                indexes: vec![],
                table_type: StTableType::User,
                table_access: StAccess::Public,
            },
        ];

        match schema_updates(current, proposed.clone())? {
            SchemaUpdates::Tainted(tainted) => bail!("unexpectedly tainted: {tainted:#?}"),
            SchemaUpdates::Updates {
                new_tables,
                indexes_to_drop,
                indexes_to_create,
            } => {
                assert!(indexes_to_drop.is_empty());
                assert!(indexes_to_create.is_empty());
                assert_eq!(new_tables.len(), 1);
                assert_eq!(new_tables.get("Pet"), proposed.last());

                Ok(())
            }
        }
    }

    #[test]
    fn test_updates_alter_indexes() -> anyhow::Result<()> {
        let current = vec![Cow::Owned(TableSchema {
            table_id: TableId(42),
            table_name: "Person".into(),
            columns: vec![
                ColumnSchema {
                    table_id: TableId(42),
                    col_id: ColId(0),
                    col_name: "id".into(),
                    col_type: AlgebraicType::U32,
                    is_autoinc: true,
                },
                ColumnSchema {
                    table_id: TableId(42),
                    col_id: ColId(1),
                    col_name: "name".into(),
                    col_type: AlgebraicType::String,
                    is_autoinc: false,
                },
            ],
            indexes: vec![IndexSchema {
                index_id: IndexId(0),
                table_id: TableId(42),
                index_name: "Person_id_unique".into(),
                is_unique: true,
                cols: NonEmpty::new(ColId(0)),
                index_type: IndexType::BTree,
            }],
            // Constraints are possibly not empty when loaded from an actual
            // database, but not inspected by `schema_updates`.
            constraints: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        })];
        let mut proposed = vec![TableDef {
            table_name: "Person".into(),
            columns: vec![
                ColumnDef {
                    col_name: "id".into(),
                    col_type: AlgebraicType::U32,
                    is_autoinc: true,
                },
                ColumnDef {
                    col_name: "name".into(),
                    col_type: AlgebraicType::String,
                    is_autoinc: false,
                },
            ],
            indexes: vec![IndexDef {
                table_id: AUTO_TABLE_ID,
                cols: NonEmpty {
                    head: ColId(0),
                    tail: vec![ColId(1)],
                },
                name: "Person_id_and_name".into(),
                is_unique: false,
                index_type: IndexType::BTree,
            }],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        }];

        match schema_updates(current, proposed.clone())? {
            SchemaUpdates::Tainted(tainted) => bail!("unexpectedly tainted: {tainted:#?}"),
            SchemaUpdates::Updates {
                new_tables,
                indexes_to_drop,
                indexes_to_create,
            } => {
                assert!(new_tables.is_empty());
                assert_eq!(indexes_to_drop.len(), 1);
                assert_eq!(indexes_to_create.len(), 1);

                assert_eq!(indexes_to_drop[0].0, 0);
                assert_eq!(
                    indexes_to_create.last(),
                    proposed[0]
                        .indexes
                        .pop()
                        .map(|mut idx| {
                            idx.table_id = TableId(42);
                            idx
                        })
                        .as_ref()
                );

                Ok(())
            }
        }
    }

    #[test]
    fn test_updates_schema_mismatch() -> anyhow::Result<()> {
        let current = vec![Cow::Owned(TableSchema {
            table_id: TableId(42),
            table_name: "Person".into(),
            columns: vec![ColumnSchema {
                table_id: TableId(42),
                col_id: ColId(0),
                col_name: "name".into(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            }],
            indexes: vec![],
            constraints: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        })];
        let proposed = vec![TableDef {
            table_name: "Person".into(),
            columns: vec![
                ColumnDef {
                    col_name: "id".into(),
                    col_type: AlgebraicType::U32,
                    is_autoinc: true,
                },
                ColumnDef {
                    col_name: "name".into(),
                    col_type: AlgebraicType::String,
                    is_autoinc: false,
                },
            ],
            indexes: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        }];

        match schema_updates(current, proposed)? {
            SchemaUpdates::Tainted(tainted) => {
                assert_eq!(tainted.len(), 1);
                assert_eq!(
                    tainted[0],
                    Tainted {
                        table_name: "Person".into(),
                        reason: TaintReason::IncompatibleSchema,
                    }
                );

                Ok(())
            }

            up @ SchemaUpdates::Updates { .. } => {
                bail!("unexpectedly not tainted: {up:#?}");
            }
        }
    }

    #[test]
    fn test_updates_orphaned_table() -> anyhow::Result<()> {
        let current = vec![Cow::Owned(TableSchema {
            table_id: TableId(42),
            table_name: "Person".into(),
            columns: vec![ColumnSchema {
                table_id: TableId(42),
                col_id: ColId(0),
                col_name: "name".into(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            }],
            indexes: vec![],
            constraints: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        })];
        let proposed = vec![TableDef {
            table_name: "Pet".into(),
            columns: vec![ColumnDef {
                col_name: "furry".into(),
                col_type: AlgebraicType::Bool,
                is_autoinc: false,
            }],
            indexes: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        }];

        match schema_updates(current, proposed)? {
            SchemaUpdates::Tainted(tainted) => {
                assert_eq!(tainted.len(), 1);
                assert_eq!(
                    tainted[0],
                    Tainted {
                        table_name: "Person".into(),
                        reason: TaintReason::Orphaned,
                    }
                );

                Ok(())
            }

            up @ SchemaUpdates::Updates { .. } => {
                bail!("unexpectedly not tainted: {up:#?}")
            }
        }
    }
}
