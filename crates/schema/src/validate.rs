// TODO(jgilles): move this to its own crate!

use itertools::Itertools;
use spacetimedb_data_structures::map::{HashMap, HashSet};

use spacetimedb_sats::{
    db::{
        column_ordering::{canonical_column_ordering, is_sorted_by},
        raw_def::{RawColumnDef, RawDatabaseDef, RawIndexDef, RawScheduleDef, RawSequenceDef, RawTableDef, IndexType, RawUniqueConstraintDef},
    },
    typespace::TypeRefError,
    AlgebraicType, AlgebraicTypeRef, ProductType, SumType, Typespace,
};
use crate::{
        def::{ColumnDef, DatabaseDef, DefLookup, IndexDef, ScheduleDef, SequenceDef, TableDef, UniqueConstraintDef},
        error::{SchemaError, SchemaErrors},
        identifier::Identifier,
};


/// Validate a schema.
/// Re-exported as `RawDatabaseDef::validate`.
pub(crate) fn validate_database(def: &RawDatabaseDef) -> Result<DatabaseDef, SchemaErrors> {
    let mut errors = SchemaErrors(Vec::new());

    // Note: this is a general pattern used in this file.
    // Validation needs to return multiple errors, so we pass in the error stream mutably,
    // and maintain the postcondition that if a function returns None, it or its callees must have pushed an error to the stream.
    let tables = def
        .tables
        .iter()
        .map(|table| validate_table(&mut errors, def, table).map(|table| (table.table_name.clone(), table)))
        .collect::<Option<HashMap<Identifier, TableDef>>>();

    if errors.0.is_empty() {
        // If there are no errors, we can safely unwrap the tables.
        // This is a postcondition of the validation functions.
        Ok(DatabaseDef {
            tables: tables.expect("No errors should mean validation succeeded"),
            // we could add additional validation here, but I don't know what would be useful to check.
            typespace: def.typespace.clone(),
        })
    } else {
        Err(errors)
    }
}

/// Validate a table.
///
/// Returns None if and only if errors have been pushed to `errors`.
fn validate_table(errors: &mut SchemaErrors, def: &RawDatabaseDef, table: &RawTableDef) -> Option<TableDef> {
    let table_name = errors.unpack(
        Identifier::new(&table.table_name).map_err(|error| SchemaError::InvalidTableName {
            table: table.table_name.clone(),
            error,
        }),
    ); // we do NOT ? here, we continue, to collect all errors possible.

    // We pass this specific thing to later validation steps.
    // That's because we need to look up *canonicalized* column names.
    let columns = table
        .columns
        .iter()
        .map(|column| validate_column(errors, def, table, column))
        .collect::<Option<Vec<ColumnDef>>>();

    let indexes = table
        .indexes
        .iter()
        .map(|index| validate_index(errors, def, table, &columns, index))
        .collect::<Option<Vec<IndexDef>>>();

    let unique_constraints = table
        .unique_constraints
        .iter()
        .map(|unique_constraint| validate_unique_constraint(errors, def, table, &columns, unique_constraint))
        .collect::<Option<Vec<UniqueConstraintDef>>>();

    let sequences = table
        .sequences
        .iter()
        .map(|sequence| validate_sequence(errors, def, table, &columns, sequence))
        .collect::<Option<Vec<SequenceDef>>>();

    // outer option: validation
    // inner option: data is optional 
    let schedule: Option<Option<ScheduleDef>> = 
    // what we really want here would be some sort of Option<Option<T>>.transpose(), but that doesn't seem to exist.
    if let Some(schedule) = &table.schedule {
        validate_schedule(errors, def, table, &columns, schedule).map(Some)
    } else {
        Some(None)
    };
    
    if let Some(columns) = &columns {
        let columns_sorted = is_sorted_by(columns, |c1, c2| {
            canonical_column_ordering(
                &def.typespace,
                (&c1.col_name, &c1.col_type),
                (&c2.col_name, &c2.col_type),
            )
        });

        if !columns_sorted {
            let mut correct = columns.clone();
            correct.sort_by(|c1, c2| {
                canonical_column_ordering(
                    &def.typespace,
                    (&c1.col_name, &c1.col_type),
                    (&c2.col_name, &c2.col_type),
                )
            });
            errors.0.push(SchemaError::TableColumnsNotOrdered {
                table: table.table_name.clone(),
                given: columns
                    .iter()
                    .map(|c| ((*c.col_name).into(), c.col_type.clone()))
                    .collect(),
                correct: correct
                    .iter()
                    .map(|c| ((*c.col_name).into(), c.col_type.clone()))
                    .collect(),
            });
            return None;
        }

        if let Some(AlgebraicType::Product(product_type)) = def.typespace.get(table.product_type_ref) {
            for (i, (product_type_element, column)) in product_type.elements.iter().zip(columns.iter()).enumerate() {
                if product_type_element.name.as_deref() != Some(&*column.col_name) {
                    errors.0.push(SchemaError::ProductTypeColumnMismatch {
                        table: table.table_name.clone(),
                        column_index: i,
                    });
                    return None;
                }
            }
        } else {
            errors.0.push(SchemaError::UninitializedProductTypeRef {
                table: table.table_name.clone(),
            });
            return None;
        }
    }

    if let (
        Some(table_name),
        Some(columns),
        Some(mut indexes),
        Some(mut unique_constraints),
        Some(mut sequences),
        Some(schedule),
    ) = (table_name, columns, indexes, unique_constraints, sequences, schedule)
    {
        #[allow(clippy::unnecessary_sort_by)]
        {
            // Can't use sort_by_key due to lifetime limitations, thanks for the workaround idea Kim
            indexes.sort_by(|index1, index2| index1.key(&table_name).cmp(&index2.key(&table_name)));
            unique_constraints.sort_by(|uc1, uc2| uc1.key(&table_name).cmp(&uc2.key(&table_name)));
            sequences.sort_by(|seq1, seq2| seq1.key(&table_name).cmp(&seq2.key(&table_name)));
        }

        Some(TableDef {
            table_name,
            columns,
            indexes,
            unique_constraints,
            sequences,
            schedule,
            table_access: table.table_access,
            table_type: table.table_type,
            product_type_ref: table.product_type_ref,
            _private: (),
        })
    } else {
        None
    }
}

/// Validate a column.
///
/// Returns None if and only if errors have been pushed to `errors`.
fn validate_column(
    errors: &mut SchemaErrors,
    def: &RawDatabaseDef,
    table: &RawTableDef,
    column: &RawColumnDef,
) -> Option<ColumnDef> {
    let col_name = errors.unpack(
        Identifier::new(&column.col_name).map_err(|error| SchemaError::InvalidColumnName {
            table: table.table_name.clone(),
            column: column.col_name.clone(),
            error,
        }),
    );
    let col_type = errors.unpack(
        check_not_recursive_and_all_refs_valid(&def.typespace, &column.col_type).map_err(|error| {
            SchemaError::InvalidColumnType {
                table: table.table_name.clone(),
                column: column.col_name.clone(),
                invalid: column.col_type.clone(),
                error,
            }
        }),
    );

    if let (Some(col_name), Some(col_type)) = (col_name, col_type) {
        Some(ColumnDef {
            col_name,
            col_type,
            _private: (),
        })
    } else {
        None
    }
}

/// Validate an index.
///
/// Returns None if and only if errors have been pushed to `errors`.
fn validate_index(
    errors: &mut SchemaErrors,
    def: &RawDatabaseDef,
    table: &RawTableDef,
    table_canonical_columns: &Option<Vec<ColumnDef>>,
    index: &RawIndexDef,
) -> Option<IndexDef> {
    let column_names = validate_column_names(errors, def, table, table_canonical_columns, &index.column_names)
        .and_then(|column_names| {
            if contains_duplicates(&column_names) {
                errors.0.push(SchemaError::IndexDefDuplicateColumnName {
                    table: table.table_name.clone(),
                    columns: column_names.iter().map(|id| (**id).into()).collect(),
                    index_type: index.index_type,
                });
                None
            } else {
                Some(column_names)
            }
        });
    if let (IndexType::BTree, Some(column_names)) = (index.index_type, column_names) {
        Some(IndexDef {
            column_names,
            index_type: index.index_type,
            _private: (),
        })
    } else {
        errors.0.push(SchemaError::OnlyBtree {
            table: table.table_name.clone(),
            column_names: index.column_names.clone(),
            index_type: index.index_type,
        });
        None
    }
}

/// Validate a unique constraint.
///
/// Returns None if and only if errors have been pushed to `errors`.
fn validate_unique_constraint(
    errors: &mut SchemaErrors,
    def: &RawDatabaseDef,
    table: &RawTableDef,
    table_canonical_columns: &Option<Vec<ColumnDef>>,
    unique_constraint: &RawUniqueConstraintDef,
) -> Option<UniqueConstraintDef> {
    let column_names = validate_column_names(
        errors,
        def,
        table,
        table_canonical_columns,
        &unique_constraint.column_names,
    );

    if let Some(mut column_names) = column_names {
        if contains_duplicates(&column_names) {
            errors.0.push(SchemaError::UniqueConstraintDefDuplicateColumnName {
                table: table.table_name.clone(),
                columns: column_names.iter().map(|id| (**id).into()).collect(),
            });
            return None;
        }
        column_names.sort();
        Some(UniqueConstraintDef {
            column_names,
            _private: (),
        })
    } else {
        None
    }
}

/// Validate a sequence.
///
/// Returns None if and only if errors have been pushed to `errors`.
fn validate_sequence(
    errors: &mut SchemaErrors,
    def: &RawDatabaseDef,
    table: &RawTableDef,
    table_canonical_columns: &Option<Vec<ColumnDef>>,
    sequence: &RawSequenceDef,
) -> Option<SequenceDef> {
    let column_name = validate_column_name(errors, table, table_canonical_columns, &sequence.column_name);

    // TODO(jgilles): validate that min_value <= start <= max_value
    if let (Some(column_name), Some(table_canonical_columns)) = (column_name, table_canonical_columns) {
        let column = table_canonical_columns
            .iter()
            .find(|column| column.col_name == column_name)
            .expect("validate_column_name guarantees column is present");

        fn is_valid(typespace: &Typespace, column_type: &AlgebraicType) -> bool {
            match column_type {
                &AlgebraicType::U8
                | &AlgebraicType::U16
                | &AlgebraicType::U32
                | &AlgebraicType::U64
                | &AlgebraicType::U128
                | &AlgebraicType::I8
                | &AlgebraicType::I16
                | &AlgebraicType::I32
                | &AlgebraicType::I64
                | &AlgebraicType::I128 => true,
                &AlgebraicType::Ref(ref_) => match typespace.get(ref_) {
                    Some(t) => is_valid(typespace, t),
                    None => false,
                },
                _ => false,
            }
        }

        if is_valid(&def.typespace, &column.col_type) {
            Some(SequenceDef {
                column_name,
                start: sequence.start,
                min_value: sequence.min_value,
                max_value: sequence.max_value,
            })
        } else {
            errors.0.push(SchemaError::InvalidSequenceColumnType {
                table: table.table_name.clone(),
                column: sequence.column_name.clone(),
                column_type: column.col_type.clone(),
            });
            None
        }
    } else {
        None
    }
}

/// Validate a schedule.
///
/// Returns None if and only if errors have been pushed to `errors`.
fn validate_schedule(
    errors: &mut SchemaErrors,
    _def: &RawDatabaseDef,
    table: &RawTableDef,
    table_canonical_columns: &Option<Vec<ColumnDef>>,
    schedule: &RawScheduleDef,
) -> Option<ScheduleDef> {
    // TODO(jgilles): validate column type here.
    let at_column = validate_column_name(errors, table, table_canonical_columns, &schedule.at_column);
    at_column.map(|at_column| ScheduleDef {
        at_column,
        // We don't validate these yet.
        reducer_name: schedule.reducer_name.clone(),
        _private: (),
    })
}

/// Validates that the column name is a valid identifier and is present in the canonical columns list, if available.
///
/// Returns None if and only if errors have been pushed to `errors`.
fn validate_column_name(
    errors: &mut SchemaErrors,
    table: &RawTableDef,
    table_canonical_columns: &Option<Vec<ColumnDef>>,
    column_name: &str,
) -> Option<Identifier> {
    let column_name = errors.unpack(
        Identifier::new(column_name).map_err(|error| SchemaError::InvalidColumnName {
            table: table.table_name.clone(),
            column: column_name.into(),
            error,
        }),
    );

    if let (Some(column_name), Some(table_canonical_columns)) = (column_name, table_canonical_columns) {
        if !table_canonical_columns
            .iter()
            .map(|column| &column.col_name)
            .contains(&column_name)
        {
            errors.0.push(SchemaError::ColumnNotFound {
                table: table.table_name.clone(),
                column: (*column_name).into(),
            });
            None
        } else {
            Some(column_name)
        }
    } else {
        None
    }
}

/// Validates that all column names are valid identifiers and are present in the canonical columns list, if available.
///
/// Returns None if and only if errors have been pushed to `errors`.
fn validate_column_names(
    errors: &mut SchemaErrors,
    _def: &RawDatabaseDef,
    table: &RawTableDef,
    table_canonical_columns: &Option<Vec<ColumnDef>>,
    column_names: &Vec<Box<str>>,
) -> Option<Vec<Identifier>> {
    let mut result = Some(Vec::new());
    for column_name in column_names {
        let column_name = validate_column_name(errors, table, table_canonical_columns, column_name);

        if let (Some(result), Some(column_name)) = (&mut result, column_name) {
            result.push(column_name);
        } else {
            result = None;
        }
    }
    result
}

/// Check if an iterator contains duplicates.
fn contains_duplicates<T: Eq + std::hash::Hash>(iter: impl IntoIterator<Item = T>) -> bool {
    let mut seen = HashSet::new();
    for item in iter {
        if !seen.insert(item) {
            return true;
        }
    }
    false
}

/// This may eventually want to live somewhere else.
fn check_not_recursive_and_all_refs_valid(
    typespace: &Typespace,
    type_: &AlgebraicType,
) -> Result<AlgebraicType, TypeRefError> {
    fn check(
        typespace: &Typespace,
        t: &AlgebraicType,
        descending_from: &[AlgebraicTypeRef],
    ) -> Result<(), TypeRefError> {
        match t {
            AlgebraicType::Product(ProductType { elements }) => {
                for element in elements.iter() {
                    check(typespace, &element.algebraic_type, descending_from)?;
                }
            }
            AlgebraicType::Sum(SumType { variants }) => {
                for variant in variants.iter() {
                    check(typespace, &variant.algebraic_type, descending_from)?;
                }
            }
            AlgebraicType::Ref(ref_) => {
                if descending_from.contains(ref_) {
                    return Err(TypeRefError::RecursiveTypeRef(*ref_));
                }

                match typespace.get(*ref_) {
                    Some(t) => {
                        let mut descending_from = descending_from.to_vec();
                        descending_from.push(*ref_);
                        check(typespace, t, &descending_from)?;
                    }
                    None => return Err(TypeRefError::InvalidTypeRef(*ref_)),
                }
            }
            _ => {}
        }
        Ok(())
    }

    check(typespace, type_, &[]).map(|_| type_.clone())
}

#[cfg(test)]
mod tests {
    use crate::def::DatabaseDef;
    use crate::error::*;
    use crate::identifier::Identifier;
    use spacetimedb_sats::db::raw_def::*;
    use spacetimedb_sats::typespace::TypeRefError;
    use spacetimedb_sats::{AlgebraicType, ProductType, ProductTypeElement, SumType, SumTypeVariant};

    macro_rules! expect_error_matching (
        ($result:expr, $expected:pat) => {
            match $result {
                Ok(_) => panic!("expected validation error"),
                Err(errors) => {
                    assert!(errors.0.iter().any(|error| matches!(error, $expected)));
                }
            }
        }
    );

    #[test]
    fn valid_definition() {
        let mut def = RawDatabaseDef::new();

        // TODO(jgilles): do we currently support this?
        let product_type = AlgebraicType::Product(ProductType {
            elements: vec![
                ProductTypeElement {
                    name: Some("a".into()),
                    algebraic_type: AlgebraicType::U64,
                },
                ProductTypeElement {
                    name: Some("b".into()),
                    algebraic_type: AlgebraicType::String,
                },
            ]
            .into_boxed_slice(),
        });

        let sum_type = AlgebraicType::Sum(SumType {
            variants: vec![
                SumTypeVariant::unit("GrannySmith"),
                SumTypeVariant::unit("Gala"),
                SumTypeVariant::unit("RedDelicious"),
            ]
            .into_boxed_slice(),
        });

        let product_type_ref = def.typespace.add(product_type.clone());
        let sum_type_ref = def.typespace.add(sum_type.clone());

        let table_1 = RawTableDef::new(
            "Bananas".into(),
            vec![
                RawColumnDef::new("id".into(), AlgebraicType::U64),
                RawColumnDef::new("name".into(), AlgebraicType::String),
                RawColumnDef::new("product_column".into(), AlgebraicType::Ref(product_type_ref)),
                RawColumnDef::new("count".into(), AlgebraicType::U16),
            ],
        )
        .with_column_sequence("id")
        .with_unique_constraint(&["id"])
        .with_index(&["id"], IndexType::BTree)
        .with_index(&["id", "name"], IndexType::BTree);

        let table_2 = RawTableDef::new(
            "Apples".into(),
            vec![
                RawColumnDef::new("id".into(), AlgebraicType::U64),
                RawColumnDef::new("name".into(), AlgebraicType::String),
                RawColumnDef::new("count".into(), AlgebraicType::U16),
                RawColumnDef::new("type".into(), AlgebraicType::Ref(sum_type_ref)),
            ],
        );
        
        let table_3 = RawTableDef::new(
            "Deliveries".into(),
            vec![
                RawColumnDef::new("id".into(), AlgebraicType::U64),
                RawColumnDef::new("at".into(), AlgebraicType::U16), // TODO(jgilles): make this a ScheduleAt enum
            ]
        ).with_schedule_def(RawScheduleDef {
            at_column: "at".into(),
            reducer_name: "check_deliveries".into(),
        });

        let def: DatabaseDef = def
            .with_table_and_product_type(table_1)
            .with_table_and_product_type(table_2)
            .with_table_and_product_type(table_3)
            .try_into()
            .expect("this should be a valid database definition");

        let apples = Identifier::new("Apples").unwrap();
        let bananas = Identifier::new("Bananas").unwrap();
        let deliveries = Identifier::new("Deliveries").unwrap();

        assert_eq!(def.tables.len(), 3);

        let apples_def = &def.tables[&apples];

        assert_eq!(apples_def.table_name, Identifier::new("Apples").unwrap());
        assert_eq!(apples_def.columns.len(), 4);
        assert_eq!(apples_def.columns[0].col_name, Identifier::new("id").unwrap());
        assert_eq!(apples_def.columns[0].col_type, AlgebraicType::U64);
        assert_eq!(apples_def.columns[1].col_name, Identifier::new("name").unwrap());
        assert_eq!(apples_def.columns[1].col_type, AlgebraicType::String);
        assert_eq!(apples_def.columns[2].col_name, Identifier::new("count").unwrap());
        assert_eq!(apples_def.columns[2].col_type, AlgebraicType::U16);
        assert_eq!(apples_def.columns[3].col_name, Identifier::new("type").unwrap());
        assert_eq!(apples_def.columns[3].col_type, AlgebraicType::Ref(sum_type_ref));

        let bananas_def = &def.tables[&bananas];

        assert_eq!(bananas_def.table_name, Identifier::new("Bananas").unwrap());
        assert_eq!(bananas_def.columns.len(), 4);
        assert_eq!(bananas_def.columns[0].col_name, Identifier::new("id").unwrap());
        assert_eq!(bananas_def.columns[0].col_type, AlgebraicType::U64);
        assert_eq!(bananas_def.columns[1].col_name, Identifier::new("name").unwrap());
        assert_eq!(bananas_def.columns[1].col_type, AlgebraicType::String);
        assert_eq!(
            bananas_def.columns[2].col_name,
            Identifier::new("product_column").unwrap()
        );
        assert_eq!(bananas_def.columns[2].col_type, AlgebraicType::Ref(product_type_ref));
        assert_eq!(bananas_def.columns[3].col_name, Identifier::new("count").unwrap());
        assert_eq!(bananas_def.columns[3].col_type, AlgebraicType::U16);

        let delivery_def = &def.tables[&deliveries];
        assert_eq!(delivery_def.table_name, Identifier::new("Deliveries").unwrap());
        assert_eq!(delivery_def.columns.len(), 2);
        assert_eq!(delivery_def.columns[0].col_name, Identifier::new("id").unwrap());
        assert_eq!(delivery_def.columns[0].col_type, AlgebraicType::U64);
        assert_eq!(delivery_def.columns[1].col_name, Identifier::new("at").unwrap());
        assert_eq!(delivery_def.columns[1].col_type, AlgebraicType::U16);
        assert_eq!(delivery_def.schedule.as_ref().unwrap().at_column, Identifier::new("at").unwrap());
        assert_eq!(&delivery_def.schedule.as_ref().unwrap().reducer_name[..], "check_deliveries");

        assert_eq!(def.typespace.types.len(), 2 + 3); // manually added 2 types, automatically initialized 2 types with add_table_and_product_type.
        assert_eq!(def.typespace.get(product_type_ref), Some(&product_type));
        assert_eq!(def.typespace.get(sum_type_ref), Some(&sum_type));

        let apples_product_type = ProductType::new(
            apples_def
                .columns
                .iter()
                .map(|c| ProductTypeElement {
                    name: Some((&*c.col_name).into()),
                    algebraic_type: c.col_type.clone(),
                })
                .collect(),
        );
        assert_eq!(
            def.typespace.get(apples_def.product_type_ref),
            Some(&AlgebraicType::Product(apples_product_type.into()))
        );

        let bananas_product_type = ProductType::new(
            bananas_def
                .columns
                .iter()
                .map(|c| ProductTypeElement {
                    name: Some((&*c.col_name).into()),
                    algebraic_type: c.col_type.clone(),
                })
                .collect(),
        );
        assert_eq!(
            def.typespace.get(bananas_def.product_type_ref),
            Some(&AlgebraicType::Product(bananas_product_type.into()))
        );

        let deliveries_product_type = ProductType::new(
            delivery_def
                .columns
                .iter()
                .map(|c| ProductTypeElement {
                    name: Some((&*c.col_name).into()),
                    algebraic_type: c.col_type.clone(),
                })
                .collect(),
        );
        assert_eq!(
            def.typespace.get(delivery_def.product_type_ref),
            Some(&AlgebraicType::Product(deliveries_product_type.into()))
        );
    }

    #[test]
    fn invalid_product_type_ref() {
        let result: Result<DatabaseDef, SchemaErrors> = RawDatabaseDef::new()
            // `with_table` does NOT initialize table.product_type_ref, which should result in an error.
            .with_table(
                RawTableDef::new(
                    "Bananas".into(),
                    vec![RawColumnDef::new("id".into(), AlgebraicType::U32)],
                )
                .with_column_sequence("id"),
            )
            .try_into();

        expect_error_matching!(result, SchemaError::UninitializedProductTypeRef { .. });
    }

    #[test]
    fn not_canonically_ordered_columns() {
        let result: Result<DatabaseDef, SchemaErrors> = RawDatabaseDef::new()
            .with_table_and_product_type(RawTableDef::new(
                "Bananas".into(),
                vec![
                    RawColumnDef::new("a".into(), AlgebraicType::U16),
                    RawColumnDef::new("b".into(), AlgebraicType::U64),
                ],
            ))
            .try_into();

        expect_error_matching!(result, SchemaError::TableColumnsNotOrdered { .. });
    }

    #[test]
    fn invalid_table_name() {
        let result: Result<DatabaseDef, SchemaErrors> = RawDatabaseDef::new()
            .with_table_and_product_type(RawTableDef::new("".into(), vec![]))
            .try_into();
        expect_error_matching!(result, SchemaError::InvalidTableName { .. });
    }

    #[test]
    fn invalid_column_name() {
        let result: Result<DatabaseDef, SchemaErrors> = RawDatabaseDef::new()
            .with_table_and_product_type(RawTableDef::new(
                "Bananas".into(),
                vec![RawColumnDef::new("".into(), AlgebraicType::U16)],
            ))
            .try_into();

        expect_error_matching!(result, SchemaError::InvalidColumnName { .. });
    }

    #[test]
    fn invalid_index_column_ref() {
        let result: Result<DatabaseDef, SchemaErrors> = RawDatabaseDef::new()
            .with_table_and_product_type(
                RawTableDef::new(
                    "Bananas".into(),
                    vec![RawColumnDef::new("id".into(), AlgebraicType::U16)],
                )
                .with_index(&["nonexistent"], IndexType::BTree),
            )
            .try_into();

        expect_error_matching!(result, SchemaError::ColumnNotFound { .. });
    }

    #[test]
    fn invalid_unique_constraint_column_ref() {
        let result: Result<DatabaseDef, SchemaErrors> = RawDatabaseDef::new()
            .with_table_and_product_type(
                RawTableDef::new(
                    "Bananas".into(),
                    vec![RawColumnDef::new("id".into(), AlgebraicType::U16)],
                )
                .with_unique_constraint(&["nonexistent"]),
            )
            .try_into();

        expect_error_matching!(result, SchemaError::ColumnNotFound { .. });
    }

    #[test]
    fn invalid_sequence_column_ref() {
        let result : Result<DatabaseDef, SchemaErrors>= RawDatabaseDef::new()
            .with_table_and_product_type(
                RawTableDef::new(
                    "Bananas".into(),
                    vec![RawColumnDef::new("id".into(), AlgebraicType::U16)],
                )
                .with_column_sequence("nonexistent"),
            )
            .try_into();

        expect_error_matching!(result, SchemaError::ColumnNotFound { .. });
    }

    #[test]
    fn invalid_index_column_duplicates() {
        let result : Result<DatabaseDef, SchemaErrors>= RawDatabaseDef::new()
            .with_table_and_product_type(
                RawTableDef::new(
                    "Bananas".into(),
                    vec![
                        RawColumnDef::new("id".into(), AlgebraicType::U16),
                        RawColumnDef::new("name".into(), AlgebraicType::String),
                    ],
                )
                .with_index(&["id", "id"], IndexType::BTree),
            )
            .try_into();

        expect_error_matching!(result, SchemaError::IndexDefDuplicateColumnName { .. });
    }

    #[test]
    fn invalid_unique_constraint_column_duplicates() {
        let result: Result<DatabaseDef, SchemaErrors> = RawDatabaseDef::new()
            .with_table_and_product_type(
                RawTableDef::new(
                    "Bananas".into(),
                    vec![
                        RawColumnDef::new("id".into(), AlgebraicType::U16),
                        RawColumnDef::new("name".into(), AlgebraicType::String),
                    ],
                )
                .with_unique_constraint(&["id", "id"]),
            )
            .try_into();

        expect_error_matching!(result, SchemaError::UniqueConstraintDefDuplicateColumnName { .. });
    }

    #[test]
    fn non_integral_sequence() {
        // wrong type
        let result: Result<DatabaseDef, SchemaErrors> = RawDatabaseDef::new()
            .with_table_and_product_type(
                RawTableDef::new(
                    "Bananas".into(),
                    vec![RawColumnDef::new("id".into(), AlgebraicType::String)],
                )
                .with_column_sequence("id"),
            )
            .try_into();

        expect_error_matching!(result, SchemaError::InvalidSequenceColumnType { .. });

        // right type behind ref (is this possible?)
        let mut def = RawDatabaseDef::new();
        let u32_ref = def.typespace.add(AlgebraicType::U32);
        let _: DatabaseDef = def.with_table_and_product_type(
            RawTableDef::new(
                "Bananas".into(),
                vec![RawColumnDef::new("id".into(), AlgebraicType::Ref(u32_ref))],
            )
            .with_column_sequence("id"),
        )
        .try_into()
        .unwrap();

        // wrong type behind ref
        let mut def = RawDatabaseDef::new();
        let u32_ref = def.typespace.add(AlgebraicType::String);
        let result: Result<DatabaseDef, SchemaErrors> = def
            .with_table_and_product_type(
                RawTableDef::new(
                    "Bananas".into(),
                    vec![RawColumnDef::new("id".into(), AlgebraicType::Ref(u32_ref))],
                )
                .with_column_sequence("id"),
            )
            .try_into();
        expect_error_matching!(result, SchemaError::InvalidSequenceColumnType { .. });
    }

    #[test]
    fn recursive_type_ref() {
        let mut def = RawDatabaseDef::new();

        let product_type = AlgebraicType::Product(ProductType {
            elements: vec![
                ProductTypeElement {
                    name: Some("a".into()),
                    algebraic_type: AlgebraicType::Ref(spacetimedb_sats::AlgebraicTypeRef(0)),
                },
                ProductTypeElement {
                    name: Some("b".into()),
                    algebraic_type: AlgebraicType::U64,
                },
            ]
            .into_boxed_slice(),
        });

        let product_type_ref = def.typespace.add(product_type);

        let result: Result<DatabaseDef, SchemaErrors> = def
            .with_table_and_product_type(
                RawTableDef::new(
                    "Bananas".into(),
                    vec![RawColumnDef::new("id".into(), AlgebraicType::Ref(product_type_ref))],
                )
                .with_column_sequence("id"),
            )
            .try_into();

        expect_error_matching!(
            result,
            SchemaError::InvalidColumnType {
                error: TypeRefError::RecursiveTypeRef(..),
                ..
            }
        );
    }

    #[test]
    fn invalid_type_ref() {
        let result : Result<DatabaseDef, SchemaErrors>= RawDatabaseDef::new()
            .with_table_and_product_type(
                RawTableDef::new(
                    "Bananas".into(),
                    vec![RawColumnDef::new(
                        "id".into(),
                        AlgebraicType::Ref(spacetimedb_sats::AlgebraicTypeRef(1)),
                    )],
                )
                .with_column_sequence("id"),
            )
            .try_into();

        expect_error_matching!(
            result,
            SchemaError::InvalidColumnType {
                error: TypeRefError::InvalidTypeRef(..),
                ..
            }
        );
    }

    #[test]
    fn only_btree_indexes() {
        let result: Result<DatabaseDef, SchemaErrors> = RawDatabaseDef::new()
            .with_table_and_product_type(
                RawTableDef::new(
                    "Bananas".into(),
                    vec![RawColumnDef::new("id".into(), AlgebraicType::U16)],
                )
                .with_index(&["id"], IndexType::Hash),
            )
            .try_into();

        expect_error_matching!(result, SchemaError::OnlyBtree { .. });
    }
}
