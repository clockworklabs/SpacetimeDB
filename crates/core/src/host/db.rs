use std::{
    collections::{BTreeMap, HashMap},
    fmt,
};

use anyhow::{ensure, Context};
use nonempty::NonEmpty;
use spacetimedb_lib::IndexType;
use spacetimedb_sats::{ProductType, Typespace};

use crate::db::datastore::{self, traits::IndexId};

/// Magic table id.
///
/// When a [`datastore::traits::TableDef`] is constructed, the corresponding
/// table does not yet exist (typically) and thus no table id is known.
///
/// Nevertheless, [`datastore::traits::IndexDef`] requires to specify a table
/// id. The magic value can be used as a placeholder, which the datastore will
/// replace with the actual value upon creation of the table.
const AUTO_TABLE_ID: u32 = 0;

/// Schema information for a single table, as extracted from an STDB module.
pub struct ModuleTableSchema<'a> {
    table_def: &'a spacetimedb_lib::TableDef,
    row_type: ProductType,
}

impl<'a> ModuleTableSchema<'a> {
    /// Resolve the row schema of the given [`spacetimedb_lib::TableDef`] in the
    /// given [`Typespace`].
    pub fn resolve(typespace: &Typespace, table_def: &'a spacetimedb_lib::TableDef) -> anyhow::Result<Self> {
        let row_type = typespace
            .with_type(&table_def.data)
            .resolve_refs()
            .context("recursive types not yet supported")?
            .into_product()
            .ok()
            .context("table not a product type?")?;
        ensure!(
            table_def.column_attrs.len() == row_type.elements.len(),
            "mismatched number of columns"
        );

        Ok(Self { table_def, row_type })
    }

    /// Hydrate this [`ModuleTableSchema`] into a full
    /// [`datastore::traits::TableDef`] suitable for creating the table.
    ///
    /// This mainly involves combining type information and declaration into
    /// [`datastore::traits::ColumnDef`]s, and determining uniqueness of (single-
    /// column) indexes from the column attributes.
    pub fn hydrate(&self) -> anyhow::Result<datastore::traits::TableDef> {
        let columns: Vec<datastore::traits::ColumnDef> =
            std::iter::zip(&self.row_type.elements, &self.table_def.column_attrs)
                .map(|(ty, attr)| {
                    Ok(datastore::traits::ColumnDef {
                        col_name: ty.name.clone().context("column without name")?,
                        col_type: ty.algebraic_type.clone(),
                        is_autoinc: attr.is_autoinc(),
                    })
                })
                .collect::<anyhow::Result<_>>()?;

        let mut indexes = Vec::new();

        // Build single-column index definitions, determining `is_unique` from their
        // respective columns attributes.
        for (col_id, col) in columns.iter().enumerate() {
            let mut index_for_column = None;
            for index in self.table_def.indexes.iter() {
                let [index_col_id] = *index.col_ids else {
                    //Ignore multi-column indexes
                    continue;
                };
                if index_col_id as usize != col_id {
                    continue;
                }
                index_for_column = Some(index);
                break;
            }

            let col_attr = self.table_def.column_attrs.get(col_id).context("invalid column id")?;
            // If there's an index defined for this column already, use it
            // making sure that it is unique if the column has a unique constraint
            if let Some(index) = index_for_column {
                match index.ty {
                    IndexType::BTree => {}
                    // TODO
                    IndexType::Hash => anyhow::bail!("hash indexes not yet supported"),
                }
                let index = datastore::traits::IndexDef {
                    table_id: AUTO_TABLE_ID,
                    cols: NonEmpty::new(col_id as u32),
                    name: index.name.clone(),
                    is_unique: col_attr.is_unique(),
                };
                indexes.push(index);
            } else if col_attr.is_unique() {
                // If you didn't find an index, but the column is unique then create
                // a unique btree index anyway.
                let index = datastore::traits::IndexDef {
                    table_id: AUTO_TABLE_ID,
                    cols: NonEmpty::new(col_id as u32),
                    name: format!("{}_{}_unique", self.table_def.name, col.col_name),
                    is_unique: true,
                };
                indexes.push(index);
            }
        }

        // Multi-column indexes cannot be unique (yet), so just add them.
        let multi_col_indexes = self.table_def.indexes.iter().filter_map(|index| {
            if index.col_ids.len() > 1 {
                Some(datastore::traits::IndexDef {
                    table_id: AUTO_TABLE_ID,
                    cols: NonEmpty::collect(index.col_ids.iter().map(|i| *i as u32))
                        .expect("empty Vec despite length check"),
                    name: index.name.clone(),
                    is_unique: false,
                })
            } else {
                None
            }
        });
        indexes.extend(multi_col_indexes);

        Ok(datastore::traits::TableDef {
            table_name: self.table_def.name.clone(),
            columns,
            indexes,
            table_type: self.table_def.table_type,
            table_access: self.table_def.table_access,
        })
    }
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
        new_tables: HashMap<String, datastore::traits::TableDef>,
        /// Indexes to drop.
        ///
        /// Should be processed _before_ `indexes_to_create`, as we might be
        /// updating (i.e. drop then create with different parameters).
        indexes_to_drop: Vec<datastore::traits::IndexId>,
        /// Indexes to create.
        ///
        /// Should be processed _after_ `indexes_to_drop`.
        indexes_to_create: Vec<datastore::traits::IndexDef>,
    },
}

/// Compute the diff between the current and proposed schema.
///
/// Compares the [`datastore::traits::TableSchema`]s loaded from the database
/// against the proposed [`datastore::traits::TableDef`]s. Both are assumed to
/// represent the full schema information of the respective source.
///
/// Tables in the latter whose schema differs from the former are returned as
/// [`SchemaUpdates::Tainted`]. Tables also become tainted if they are
/// no longer present in the proposed schema (they are said to be "orphaned"),
/// although this restriction may be lifted in the future.
///
/// If no tables become tainted, the database may safely be updated using the
/// information in [`SchemaUpdates::Updates`].
pub fn schema_updates(
    current: impl IntoIterator<Item = datastore::traits::TableSchema>,
    proposed: impl IntoIterator<Item = anyhow::Result<datastore::traits::TableDef>>,
) -> anyhow::Result<SchemaUpdates> {
    let mut new_tables = HashMap::new();
    let mut tainted_tables = Vec::new();
    let mut indexes_to_create = Vec::new();
    let mut indexes_to_drop = Vec::new();

    let mut known_tables: BTreeMap<String, datastore::traits::TableSchema> = current
        .into_iter()
        .map(|schema| (schema.table_name.clone(), schema))
        .collect();

    for proposed_schema_def in proposed {
        let proposed_schema_def = proposed_schema_def?;

        let proposed_table_name = &proposed_schema_def.table_name;
        if let Some(known_schema) = known_tables.remove(proposed_table_name) {
            let table_id = known_schema.table_id;
            let known_schema_def = datastore::traits::TableDef::from(known_schema.clone());
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
                    .into_iter()
                    .map(|idx| (idx.index_name.clone(), idx))
                    .collect::<BTreeMap<_, _>>();

                for mut index_def in proposed_schema_def.indexes {
                    // This is zero in the proposed schema, as the table id
                    // is not known at proposal time.
                    index_def.table_id = table_id;

                    match known_indexes.remove(&index_def.name) {
                        None => indexes_to_create.push(index_def),
                        Some(known_index) => {
                            let known_id = IndexId(known_index.index_id);
                            let known_index_def = datastore::traits::IndexDef::from(known_index);
                            if known_index_def != index_def {
                                indexes_to_drop.push(known_id);
                                indexes_to_create.push(index_def);
                            }
                        }
                    }
                }

                // Indexes not in the proposed schema shall be dropped.
                for index in known_indexes.into_values() {
                    indexes_to_drop.push(IndexId(index.index_id));
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
fn equiv(a: &datastore::traits::TableDef, b: &datastore::traits::TableDef) -> bool {
    let datastore::traits::TableDef {
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
    use spacetimedb_lib::{
        auth::{StAccess, StTableType},
        ColumnIndexAttribute,
    };
    use spacetimedb_sats::{AlgebraicType, AlgebraicTypeRef, ProductTypeElement};

    use super::*;

    #[test]
    fn test_hydrate() -> anyhow::Result<()> {
        let module_schema = ModuleTableSchema {
            table_def: &spacetimedb_lib::TableDef {
                name: "Person".into(),
                // Dummy value - we supply the row type manually
                data: AlgebraicTypeRef(42),
                column_attrs: vec![ColumnIndexAttribute::IDENTITY, ColumnIndexAttribute::UNSET],
                indexes: vec![
                    spacetimedb_lib::IndexDef {
                        name: "id_and_name".into(),
                        ty: spacetimedb_lib::IndexType::BTree,
                        col_ids: vec![0, 1],
                    },
                    spacetimedb_lib::IndexDef {
                        name: "just_name".into(),
                        ty: spacetimedb_lib::IndexType::BTree,
                        col_ids: vec![1],
                    },
                ],
                table_type: StTableType::User,
                table_access: StAccess::Public,
            },
            row_type: ProductType::new(vec![
                ProductTypeElement {
                    name: Some("id".into()),
                    algebraic_type: AlgebraicType::U32,
                },
                ProductTypeElement {
                    name: Some("name".into()),
                    algebraic_type: AlgebraicType::String,
                },
            ]),
        };

        let mut datastore_schema = module_schema.hydrate()?;
        let mut expected_schema = datastore::traits::TableDef {
            table_name: "Person".into(),
            columns: vec![
                datastore::traits::ColumnDef {
                    col_name: "id".into(),
                    col_type: AlgebraicType::U32,
                    is_autoinc: true,
                },
                datastore::traits::ColumnDef {
                    col_name: "name".into(),
                    col_type: AlgebraicType::String,
                    is_autoinc: false,
                },
            ],
            indexes: vec![
                datastore::traits::IndexDef {
                    table_id: AUTO_TABLE_ID,
                    cols: NonEmpty::new(0),
                    name: "Person_id_unique".into(),
                    is_unique: true,
                },
                datastore::traits::IndexDef {
                    table_id: AUTO_TABLE_ID,
                    cols: NonEmpty { head: 0, tail: vec![1] },
                    name: "id_and_name".into(),
                    is_unique: false,
                },
                datastore::traits::IndexDef {
                    table_id: AUTO_TABLE_ID,
                    cols: NonEmpty::new(1),
                    name: "just_name".into(),
                    is_unique: false,
                },
            ],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        };

        for schema in [&mut datastore_schema, &mut expected_schema] {
            schema.columns.sort_by(|a, b| a.col_name.cmp(&b.col_name));
            schema.indexes.sort_by(|a, b| a.name.cmp(&b.name));
        }

        assert_eq!(expected_schema, datastore_schema);

        Ok(())
    }

    #[test]
    fn test_updates_new_table() -> anyhow::Result<()> {
        let current = [datastore::traits::TableSchema {
            table_id: 42,
            table_name: "Person".into(),
            columns: vec![datastore::traits::ColumnSchema {
                table_id: 42,
                col_id: 0,
                col_name: "name".into(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            }],
            indexes: vec![],
            constraints: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        }];
        let proposed = [
            datastore::traits::TableDef {
                table_name: "Person".into(),
                columns: vec![datastore::traits::ColumnDef {
                    col_name: "name".into(),
                    col_type: AlgebraicType::String,
                    is_autoinc: false,
                }],
                indexes: vec![],
                table_type: StTableType::User,
                table_access: StAccess::Public,
            },
            datastore::traits::TableDef {
                table_name: "Pet".into(),
                columns: vec![datastore::traits::ColumnDef {
                    col_name: "furry".into(),
                    col_type: AlgebraicType::Bool,
                    is_autoinc: false,
                }],
                indexes: vec![],
                table_type: StTableType::User,
                table_access: StAccess::Public,
            },
        ];

        match schema_updates(current, proposed.iter().cloned().map(Ok))? {
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
        let current = [datastore::traits::TableSchema {
            table_id: 42,
            table_name: "Person".into(),
            columns: vec![
                datastore::traits::ColumnSchema {
                    table_id: 42,
                    col_id: 0,
                    col_name: "id".into(),
                    col_type: AlgebraicType::U32,
                    is_autoinc: true,
                },
                datastore::traits::ColumnSchema {
                    table_id: 42,
                    col_id: 1,
                    col_name: "name".into(),
                    col_type: AlgebraicType::String,
                    is_autoinc: false,
                },
            ],
            indexes: vec![datastore::traits::IndexSchema {
                index_id: 0,
                table_id: 42,
                index_name: "Person_id_unique".into(),
                is_unique: true,
                cols: NonEmpty::new(0),
            }],
            // Constraints are possibly not empty when loaded from an actual
            // database, but not inspected by `schema_updates`.
            constraints: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        }];
        let mut proposed = vec![datastore::traits::TableDef {
            table_name: "Person".into(),
            columns: vec![
                datastore::traits::ColumnDef {
                    col_name: "id".into(),
                    col_type: AlgebraicType::U32,
                    is_autoinc: true,
                },
                datastore::traits::ColumnDef {
                    col_name: "name".into(),
                    col_type: AlgebraicType::String,
                    is_autoinc: false,
                },
            ],
            indexes: vec![datastore::traits::IndexDef {
                table_id: AUTO_TABLE_ID,
                cols: NonEmpty { head: 0, tail: vec![1] },
                name: "Person_id_and_name".into(),
                is_unique: false,
            }],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        }];

        match schema_updates(current, proposed.iter().cloned().map(Ok))? {
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
                            idx.table_id = 42;
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
        let current = [datastore::traits::TableSchema {
            table_id: 42,
            table_name: "Person".into(),
            columns: vec![datastore::traits::ColumnSchema {
                table_id: 42,
                col_id: 0,
                col_name: "name".into(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            }],
            indexes: vec![],
            constraints: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        }];
        let proposed = [Ok(datastore::traits::TableDef {
            table_name: "Person".into(),
            columns: vec![
                datastore::traits::ColumnDef {
                    col_name: "id".into(),
                    col_type: AlgebraicType::U32,
                    is_autoinc: true,
                },
                datastore::traits::ColumnDef {
                    col_name: "name".into(),
                    col_type: AlgebraicType::String,
                    is_autoinc: false,
                },
            ],
            indexes: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        })];

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
        let current = [datastore::traits::TableSchema {
            table_id: 42,
            table_name: "Person".into(),
            columns: vec![datastore::traits::ColumnSchema {
                table_id: 42,
                col_id: 0,
                col_name: "name".into(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            }],
            indexes: vec![],
            constraints: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        }];
        let proposed = [Ok(datastore::traits::TableDef {
            table_name: "Pet".into(),
            columns: vec![datastore::traits::ColumnDef {
                col_name: "furry".into(),
                col_type: AlgebraicType::Bool,
                is_autoinc: false,
            }],
            indexes: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        })];

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
