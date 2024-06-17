// TODO(jgilles): move this to its own crate!

use std::fmt;

use crate::{def::*, identifier::Identifier};
use spacetimedb_sats::db::auth::StTableType;
use spacetimedb_sats::AlgebraicType;

/// A plan for an automatic migration.
#[derive(Debug)]
pub struct AutoMigratePlan<'def> {
    /// The old database definition.
    pub old: &'def DatabaseDef,
    /// The new database definition.
    pub new: &'def DatabaseDef,
    /// The checks to perform before the automatic migration.
    pub prechecks: Vec<AutoMigratePrecheck>,
    /// The migration steps to perform.
    /// Order should not matter, as the steps are independent.
    pub steps: Vec<AutoMigrateStep>,
}

/// Checks that must be performed before performing an automatic migration.
/// These checks can access table contents and other database state.
#[derive(PartialEq, Eq, Debug)]
pub enum AutoMigratePrecheck {
    /// Perform a check that adding a sequence is valid (the relevant column contains no values
    /// greater than the sequence's start value).
    CheckAddSequenceRangeValid(<SequenceDef as DefLookup>::Key),
}

/// A step in an automatic migration.
#[derive(PartialEq, Eq, Debug)]
pub enum AutoMigrateStep {
    /// Add a table, including all indexes, constraints, and sequences.
    /// There will NOT be separate steps in the plan for adding indexes, constraints, and sequences.
    AddTable(<TableDef as DefLookup>::Key),
    /// Add an index.
    AddIndex(<IndexDef as DefLookup>::Key),
    /// Remove an index.
    RemoveIndex(<IndexDef as DefLookup>::Key),
    /// Remove a unique constraint.
    RemoveUniqueConstraint(<UniqueConstraintDef as DefLookup>::Key),
    /// Add a sequence.
    AddSequence(<SequenceDef as DefLookup>::Key),
    /// Remove a sequence.
    RemoveSequence(<SequenceDef as DefLookup>::Key),
    /// Change the access of a table.
    ChangeAccess(<TableDef as DefLookup>::Key),
    /// Add a schedule annotation to a table.
    AddSchedule(<ScheduleDef as DefLookup>::Key),
    /// Remove a schedule annotation from a table.
    RemoveSchedule(<ScheduleDef as DefLookup>::Key),
}

/// Something that might prevent an automatic migration.
#[derive(thiserror::Error, Debug, PartialEq)]
pub enum AutoMigrateError {
    #[error("Adding a column {column} to table {table} requires a manual migration")]
    AddColumnRequiresManualMigrate { table: Identifier, column: Identifier },

    #[error("Removing a column {column} from table {table} requires a manual migration")]
    RemoveColumnRequiresManualMigrate { table: Identifier, column: Identifier },

    #[error(
        "Changing the type of column {column} in table {table} from {type1:?} to {type2:?} requires a manual migration"
    )]
    ChangeColumnTypeRequiresManualMigrate {
        table: Identifier,
        column: Identifier,
        type1: AlgebraicType,
        type2: AlgebraicType,
    },

    #[error("Adding a unique constraint on columns {columns:?} to table {table} requires a manual migration")]
    AddUniqueConstraintRequiresManualMigrate {
        table: Identifier,
        columns: Vec<Identifier>,
    },

    #[error("Removing the table {table} requires a manual migration")]
    RemoveTableRequiresManualMigrate { table: Identifier },

    #[error("Changing the table type of table {table} from {type1:?} to {type2:?} requires a manual migration")]
    ChangeTableTypeRequiresManualMigrate {
        table: Identifier,
        type1: StTableType,
        type2: StTableType,
    },
}

/// A stream of automatic migration errors.
#[derive(thiserror::Error, Debug, PartialEq)]
pub struct AutoMigrateErrors(pub Vec<AutoMigrateError>);

impl fmt::Display for AutoMigrateErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.0.iter()).finish()
    }
}

impl AutoMigrateErrors {
    /// Unpacks a result into the error stream, returning the value if it is Ok.
    pub(crate) fn unpack<T>(&mut self, result: Result<T, AutoMigrateError>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(err) => {
                self.0.push(err);
                None
            }
        }
    }
}

/// Construct an automatic migration plan, or reject with reasons why automatic migration can't be performed.
pub fn ponder_automigrate<'def>(
    old: &'def DatabaseDef,
    new: &'def DatabaseDef,
) -> Result<AutoMigratePlan<'def>, AutoMigrateErrors> {
    // Both the old and new database definitions have already been validated (this is enforced by the types).
    // All we have to do is walk through and compare them.
    let mut errors = AutoMigrateErrors(Vec::new());
    let mut plan = AutoMigratePlan {
        old,
        new,
        steps: Vec::new(),
        prechecks: Vec::new(),
    };
    let mut old_typespace_mut = old.typespace.clone();
    let mut new_typespace_mut = new.typespace.clone();

    for old_table in old.tables.values() {
        let old_table_key = old_table.key(&());

        let new_table = errors.unpack(new.tables.get(&old_table_key).ok_or_else(|| {
            AutoMigrateError::RemoveTableRequiresManualMigrate {
                table: old_table_key.clone(),
            }
        }));
        let new_table = match new_table {
            Some(new_table) => new_table,
            None => continue,
        };

        if old_table.table_type != new_table.table_type {
            errors.0.push(AutoMigrateError::ChangeTableTypeRequiresManualMigrate {
                table: old_table_key.clone(),
                type1: old_table.table_type,
                type2: new_table.table_type,
            });
        }
        if old_table.table_access != new_table.table_access {
            plan.steps.push(AutoMigrateStep::ChangeAccess(old_table_key.clone()));
        }

        let mut column_errors = false;
        for old_col in &old_table.columns {
            let old_col_key = old_col.key(&old_table_key);

            let new_col = errors.unpack(ColumnDef::lookup(new, &old_col_key).ok_or_else(|| {
                column_errors = true;
                AutoMigrateError::RemoveColumnRequiresManualMigrate {
                    table: old_table.table_name.clone(),
                    column: old_col.col_name.clone(),
                }
            }));
            let new_col = match new_col {
                Some(new_col) => new_col,
                None => continue,
            };
            if old_col.col_type != new_col.col_type {
                column_errors = true;
                errors.0.push(AutoMigrateError::ChangeColumnTypeRequiresManualMigrate {
                    table: old_table.table_name.clone(),
                    column: old_col.col_name.clone(),
                    type1: old_col.col_type.clone(),
                    type2: new_col.col_type.clone(),
                });
            }
        }
        for new_col in &new_table.columns {
            let new_col_key = new_col.key(&old_table_key);

            match old_table
                .columns
                .iter()
                .find(|old_col| old_col.key(&old_table_key) == new_col_key)
            {
                Some(_) => continue,
                None => {
                    column_errors = true;
                    errors.0.push(AutoMigrateError::AddColumnRequiresManualMigrate {
                        table: old_table_key.clone(),
                        column: new_col.col_name.clone(),
                    })
                }
            }
        }
        if !column_errors {
            let mut old_product_type = old_typespace_mut
                .get(old_table.product_type_ref)
                .expect("valid TableDef must have valid product_type_ref")
                .clone();
            let mut new_product_type = new_typespace_mut
                .get(new_table.product_type_ref)
                .expect("valid TableDef must have valid product_type_ref")
                .clone();
            old_typespace_mut
                .inline_typerefs_in_type(&mut old_product_type)
                .expect("valid TableDef must have acyclic product_type_ref");
            new_typespace_mut
                .inline_typerefs_in_type(&mut new_product_type)
                .expect("valid TableDef must have acyclic product_type_ref");
            // Assert: this is a sanity check.
            // It should always succeed, because:
            // - both DatabaseDefs are valid
            // - => both TableDefs are valid
            // - => both product_type_refs are valid
            // - and we have checked for all possible changes in the TableDefs
            //   that would cause the constructed product types to be structurally different.
            //   (after inlining of type refs.)
            assert_eq!(
                old_product_type, new_product_type,
                "If all columns are the same, the types ({}, {}) should be the same (in table {})",
                old_table.product_type_ref, new_table.product_type_ref, old_table_key
            );
        }

        for old_index in &old_table.indexes {
            let old_index_key = old_index.key(&old_table_key);

            match IndexDef::lookup(new, &old_index_key) {
                Some(_) => continue,
                None => plan.steps.push(AutoMigrateStep::RemoveIndex(old_index_key.clone())),
            }
        }
        for new_index in &new_table.indexes {
            let new_index_key = new_index.key(&old_table_key);

            match IndexDef::lookup(old, &new_index_key) {
                Some(_) => continue,
                None => plan.steps.push(AutoMigrateStep::AddIndex(new_index_key.clone())),
            }
        }

        for old_sequence in &old_table.sequences {
            let old_sequence_key = old_sequence.key(&old_table_key);

            match SequenceDef::lookup(new, &old_sequence_key) {
                Some(_) => continue,
                None => plan
                    .steps
                    .push(AutoMigrateStep::RemoveSequence(old_sequence_key.clone())),
            }
        }
        for new_sequence in &new_table.sequences {
            let new_sequence_key = new_sequence.key(&old_table_key);

            match SequenceDef::lookup(old, &new_sequence_key) {
                Some(_) => continue,
                None => {
                    plan.prechecks.push(AutoMigratePrecheck::CheckAddSequenceRangeValid(
                        new_sequence_key.clone(),
                    ));
                    plan.steps.push(AutoMigrateStep::AddSequence(new_sequence_key.clone()));
                }
            }
        }

        for old_unique_constraint in &old_table.unique_constraints {
            let old_unique_constraint_key = old_unique_constraint.key(&old_table_key);

            match UniqueConstraintDef::lookup(new, &old_unique_constraint_key) {
                Some(_) => continue,
                None => plan.steps.push(AutoMigrateStep::RemoveUniqueConstraint(
                    old_unique_constraint_key.clone(),
                )),
            }
        }
        for new_unique_constraint in &new_table.unique_constraints {
            let new_unique_constraint_key = new_unique_constraint.key(&old_table_key);

            match UniqueConstraintDef::lookup(old, &new_unique_constraint_key) {
                Some(_) => continue,
                None => errors
                    .0
                    .push(AutoMigrateError::AddUniqueConstraintRequiresManualMigrate {
                        table: old_table_key.clone(),
                        columns: new_unique_constraint.column_names.clone(),
                    }),
            }
        }

        if old_table.schedule != new_table.schedule {
            // Note: this handles the case where there's an altered ScheduleDef for some reason.
            if old_table.schedule.is_some() {
                plan.steps.push(AutoMigrateStep::RemoveSchedule(old_table_key.clone()));
            }
            if new_table.schedule.is_some() {
                plan.steps.push(AutoMigrateStep::AddSchedule(old_table_key.clone()));
            }
        }
    }
    for new_table in new.tables.values() {
        let new_table_key = new_table.key(&());

        match old.tables.get(&new_table_key) {
            Some(_) => continue,
            None => plan.steps.push(AutoMigrateStep::AddTable(new_table_key.clone())),
        }
    }

    if errors.0.is_empty() {
        Ok(plan)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_sats::db::{auth::StAccess, raw_def::*};

    #[test]
    fn successful_auto_migration() {
        let old_def: DatabaseDef = RawDatabaseDef::new()
            .with_table_and_product_type(
                RawTableDef::new(
                    "Apples".into(),
                    vec![
                        RawColumnDef::new("id".into(), AlgebraicType::U64),
                        RawColumnDef::new("name".into(), AlgebraicType::String),
                        RawColumnDef::new("count".into(), AlgebraicType::U16),
                    ],
                )
                .with_column_sequence("id")
                .with_unique_constraint(&["id"])
                .with_index(&["id"], IndexType::BTree)
                .with_index(&["id", "name"], IndexType::BTree),
            )
            .with_table_and_product_type(RawTableDef::new(
                "Bananas".into(),
                vec![
                    RawColumnDef::new("id".into(), AlgebraicType::U64),
                    RawColumnDef::new("name".into(), AlgebraicType::String),
                    RawColumnDef::new("count".into(), AlgebraicType::U16),
                ],
            ))
            .with_table_and_product_type(
                RawTableDef::new(
                    "Deliveries".into(),
                    vec![
                        RawColumnDef::new("id".into(), AlgebraicType::U64),
                        RawColumnDef::new("at".into(), AlgebraicType::U16), // TODO(jgilles): make this a ScheduleAt enum
                    ],
                )
                .with_schedule_def(RawScheduleDef {
                    at_column: "at".into(),
                    reducer_name: "check_deliveries".into(),
                }),
            )
            .with_table_and_product_type(RawTableDef::new(
                "Inspections".into(),
                vec![
                    RawColumnDef::new("id".into(), AlgebraicType::U64),
                    RawColumnDef::new("at".into(), AlgebraicType::U16), // TODO(jgilles): make this a ScheduleAt enum
                ],
            ))
            .try_into()
            .expect("old_def should be a valid database definition");

        let new_def: DatabaseDef = RawDatabaseDef::new()
            .with_table_and_product_type(
                RawTableDef::new(
                    "Apples".into(),
                    vec![
                        RawColumnDef::new("id".into(), AlgebraicType::U64),
                        RawColumnDef::new("name".into(), AlgebraicType::String),
                        RawColumnDef::new("count".into(), AlgebraicType::U16),
                    ],
                )
                // remove sequence
                // remove unique constraint
                .with_index(&["id"], IndexType::BTree)
                // remove ["id", "name"] index
                .with_index(&["name"], IndexType::BTree), // add index
            )
            .with_table_and_product_type(
                RawTableDef::new(
                    "Bananas".into(),
                    vec![
                        RawColumnDef::new("id".into(), AlgebraicType::U64),
                        RawColumnDef::new("name".into(), AlgebraicType::String),
                        RawColumnDef::new("count".into(), AlgebraicType::U16),
                    ],
                )
                .with_column_sequence("id") // add column sequence
                .with_access(StAccess::Private), // change access
            )
            .with_table_and_product_type(
                RawTableDef::new(
                    "Oranges".into(),
                    vec![RawColumnDef::new("id".into(), AlgebraicType::U64)],
                )
                .with_index(&["id"], IndexType::BTree)
                .with_column_sequence("id")
                .with_unique_constraint(&["id"]),
            ) // add one table with the works
            .with_table_and_product_type(
                RawTableDef::new(
                    "Deliveries".into(),
                    vec![
                        RawColumnDef::new("id".into(), AlgebraicType::U64),
                        RawColumnDef::new("at".into(), AlgebraicType::U16), // TODO(jgilles): make this a ScheduleAt enum
                    ],
                ), // remove schedule def
            )
            .with_table_and_product_type(
                RawTableDef::new(
                    "Inspections".into(),
                    vec![
                        RawColumnDef::new("id".into(), AlgebraicType::U64),
                        RawColumnDef::new("at".into(), AlgebraicType::U16), // TODO(jgilles): make this a ScheduleAt enum
                    ],
                )
                .with_schedule_def(RawScheduleDef {
                    at_column: "at".into(),
                    reducer_name: "perform_inspection".into(),
                }), // add schedule def
            )
            .try_into()
            .expect("new_def should be a valid database definition");

        let plan = ponder_automigrate(&old_def, &new_def).expect("auto migration should succeed");

        let apples = Identifier::new("Apples").unwrap();
        let bananas = Identifier::new("Bananas").unwrap();
        let oranges = Identifier::new("Oranges").unwrap();
        let deliveries = Identifier::new("Deliveries").unwrap();
        let inspections = Identifier::new("Inspections").unwrap();

        let old_apples = &old_def.tables[&apples];
        let old_bananas = &old_def.tables[&bananas];

        let new_apples = &new_def.tables[&apples];
        let new_bananas = &new_def.tables[&bananas];

        assert_eq!(plan.prechecks.len(), 1);
        assert_eq!(
            plan.prechecks[0],
            AutoMigratePrecheck::CheckAddSequenceRangeValid(new_bananas.sequences[0].key(&bananas))
        );

        assert!(plan
            .steps
            .contains(&AutoMigrateStep::RemoveSequence(old_apples.sequences[0].key(&apples))));
        assert!(plan.steps.contains(&AutoMigrateStep::RemoveUniqueConstraint(
            old_apples.unique_constraints[0].key(&apples)
        )));
        assert!(plan
            .steps
            .contains(&AutoMigrateStep::RemoveIndex(old_apples.indexes[1].key(&apples))));
        assert!(plan
            .steps
            .contains(&AutoMigrateStep::AddIndex(new_apples.indexes[1].key(&apples))));

        assert!(plan
            .steps
            .contains(&AutoMigrateStep::ChangeAccess(old_bananas.key(&()))));
        assert!(plan
            .steps
            .contains(&AutoMigrateStep::AddSequence(new_bananas.sequences[0].key(&bananas))));

        assert!(plan.steps.contains(&AutoMigrateStep::AddTable(oranges.clone())));

        assert!(plan
            .steps
            .contains(&AutoMigrateStep::RemoveSchedule(deliveries.clone())));
        assert!(plan.steps.contains(&AutoMigrateStep::AddSchedule(inspections.clone())));
    }

    #[test]
    fn auto_migration_errors() {
        let old_def: DatabaseDef = RawDatabaseDef::new()
            .with_table_and_product_type(
                RawTableDef::new(
                    "Apples".into(),
                    vec![
                        RawColumnDef::new("id".into(), AlgebraicType::U64),
                        RawColumnDef::new("name".into(), AlgebraicType::String),
                        RawColumnDef::new("count".into(), AlgebraicType::U16),
                    ],
                )
                .with_type(StTableType::User),
            )
            .with_table_and_product_type(RawTableDef::new(
                "Bananas".into(),
                vec![
                    RawColumnDef::new("id".into(), AlgebraicType::U64),
                    RawColumnDef::new("name".into(), AlgebraicType::String),
                    RawColumnDef::new("count".into(), AlgebraicType::U16),
                ],
            ))
            .try_into()
            .expect("old_def should be a valid database definition");

        let new_def: DatabaseDef = RawDatabaseDef::new()
            .with_table_and_product_type(
                RawTableDef::new(
                    "Apples".into(),
                    vec![
                        RawColumnDef::new("id".into(), AlgebraicType::U64),
                        RawColumnDef::new("name".into(), AlgebraicType::U32), // change type of `name`
                        // remove count
                        RawColumnDef::new("weight".into(), AlgebraicType::U16), // add weight
                    ],
                )
                .with_unique_constraint(&["id"]) // add unique constraint
                .with_type(StTableType::System), // change type
            )
            // remove Bananas
            .try_into()
            .expect("new_def should be a valid database definition");

        let errors = ponder_automigrate(&old_def, &new_def).unwrap_err();

        assert!(errors.0.contains(&AutoMigrateError::AddColumnRequiresManualMigrate {
            table: Identifier::new("Apples").unwrap(),
            column: Identifier::new("weight").unwrap()
        }));
        assert!(errors.0.contains(&AutoMigrateError::RemoveColumnRequiresManualMigrate {
            table: Identifier::new("Apples").unwrap(),
            column: Identifier::new("count").unwrap()
        }));
        assert!(errors
            .0
            .contains(&AutoMigrateError::ChangeColumnTypeRequiresManualMigrate {
                table: Identifier::new("Apples").unwrap(),
                column: Identifier::new("name").unwrap(),
                type1: AlgebraicType::String,
                type2: AlgebraicType::U32
            }));

        assert!(errors
            .0
            .contains(&AutoMigrateError::AddUniqueConstraintRequiresManualMigrate {
                table: Identifier::new("Apples").unwrap(),
                columns: vec![Identifier::new("id").unwrap()]
            }));

        assert!(errors
            .0
            .contains(&AutoMigrateError::ChangeTableTypeRequiresManualMigrate {
                table: Identifier::new("Apples").unwrap(),
                type1: StTableType::User,
                type2: StTableType::System
            }));

        assert!(errors.0.contains(&AutoMigrateError::RemoveTableRequiresManualMigrate {
            table: Identifier::new("Bananas").unwrap()
        }));
    }
}
