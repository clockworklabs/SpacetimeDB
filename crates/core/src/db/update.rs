use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};

use anyhow::Context;
use spacetimedb_lib::Hash;

use crate::database_logger::SystemLogger;
use crate::error::DBError;

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

pub fn update_database(
    stdb: &RelationalDB,
    tx: MutTxId,
    proposed_tables: Vec<TableDef>,
    fence: u128,
    module_hash: Hash,
    system_logger: &mut SystemLogger,
) -> anyhow::Result<Result<MutTxId, UpdateDatabaseError>> {
    let (tx, res) = stdb.with_auto_rollback::<_, _, anyhow::Error>(tx, |tx| {
        let existing_tables = stdb.get_all_tables(tx)?;
        let updates = crate::db::update::schema_updates(existing_tables, proposed_tables, system_logger)?;

        if updates.tainted_tables.is_empty() {
            for (name, schema) in updates.new_tables {
                system_logger.info(&format!("creating table `{}`", name));
                stdb.create_table(tx, schema)
                    .with_context(|| format!("failed to create table {}", name))?;
            }

            for index_id in updates.indexes_to_drop {
                system_logger.info(&format!("dropping index with id {}", index_id.0));
                stdb.drop_index(tx, index_id)?;
            }

            for index_def in updates.indexes_to_create {
                system_logger.info(&format!("creating index `{}`", index_def.name));
                stdb.create_index(tx, index_def)?;
            }
        } else {
            system_logger.error("module update rejected due to schema mismatch");
            return Ok(Err(UpdateDatabaseError::IncompatibleSchema {
                tables: updates.tainted_tables,
            }));
        }

        // Update the module hash. Morally, this should be done _after_ calling
        // the `update` reducer, but that consumes our transaction context.
        stdb.set_program_hash(tx, fence, module_hash)?;

        Ok(Ok(()))
    })?;
    Ok(stdb.rollback_on_err(tx, res).map(|(tx, ())| tx))
}

/// Compute the diff between the current and proposed schema.
fn schema_updates(
    existing_tables: Vec<Cow<'_, TableSchema>>,
    proposed_tables: Vec<TableDef>,
    system_logger: &mut SystemLogger,
) -> anyhow::Result<SchemaUpdates> {
    // Until we know how to migrate schemas, we only accept `TableDef`s for
    // existing tables which are equal sans their indexes.
    fn tables_equiv(a: &TableDef, b: &TableDef) -> bool {
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

    let mut new_tables = HashMap::new();
    let mut tainted_tables = Vec::new();
    let mut indexes_to_create = Vec::new();
    let mut indexes_to_drop = Vec::new();

    let mut known_tables: BTreeMap<_, _> = existing_tables
        .into_iter()
        .map(|schema| (schema.table_name.clone(), schema))
        .collect();

    for proposed_schema_def in proposed_tables {
        let table_name = &proposed_schema_def.table_name;
        let Some(known_schema) = known_tables.remove(table_name) else {
            new_tables.insert(table_name.to_owned(), proposed_schema_def);
            continue;
        };
        let table_id = known_schema.table_id;
        let known_schema_def = TableDef::from(&*known_schema);

        // If the schemas differ acc. to `tables_equiv`, the update should be rejected.
        if !tables_equiv(&known_schema_def, &proposed_schema_def) {
            system_logger.warn(&format!("stored and proposed schema of `{table_name}` differ"));
            tainted_tables.push(table_name.to_owned());
            continue;
        }

        // The schema is unchanged, but maybe the indexes are.
        let mut known_indexes = known_schema
            .indexes
            .iter()
            .map(|idx| (&idx.index_name, idx))
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
    // We may at some point decide to drop orphaned tables automatically,
    // but for now it's an incompatible schema change
    for orphan in known_tables.into_keys() {
        if !orphan.starts_with("st_") {
            system_logger.warn(format!("Orphaned table: {}", orphan).as_str());
            tainted_tables.push(orphan);
        }
    }

    Ok(SchemaUpdates {
        new_tables,
        tainted_tables,
        indexes_to_drop,
        indexes_to_create,
    })
}

struct SchemaUpdates {
    /// Tables to create.
    new_tables: HashMap<String, TableDef>,
    /// Names of tables with incompatible schema updates.
    tainted_tables: Vec<String>,
    /// Indexes to drop.
    ///
    /// Should be processed _before_ `indexes_to_create`, as we might be
    /// updating (i.e. drop then create with different parameters).
    indexes_to_drop: Vec<IndexId>,
    /// Indexes to create.
    ///
    /// Should be processed _after_ `indexes_to_drop`.
    indexes_to_create: Vec<IndexDef>,
}
