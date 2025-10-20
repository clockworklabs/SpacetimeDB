//! Backwards-compatibility for the previous version of the schema definition format.
//! This will be removed before 1.0.

use crate::def::{validate::Result, ModuleDef};
use crate::error::{RawColumnName, ValidationError, ValidationErrors};
use spacetimedb_data_structures::map::HashSet;
use spacetimedb_lib::db::raw_def::{v8::*, v9::*};
use spacetimedb_lib::{
    // TODO: rename these types globally in a followup PR
    AlgebraicType,
    MiscModuleExport as RawMiscModuleExportV8,
    ProductType,
    ReducerDef as RawReducerDefV8,
    TableDesc as RawTableDescV8,
    TypeAlias as RawTypeAliasV8,
};
use spacetimedb_primitives::{ColId, ColList, ConstraintKind, Constraints};
use spacetimedb_sats::{AlgebraicTypeRef, Typespace, WithTypespace};

const INIT_NAME: &str = "__init__";
const IDENTITY_CONNECTED_NAME: &str = "__identity_connected__";
const IDENTITY_DISCONNECTED_NAME: &str = "__identity_disconnected__";

/// Validate a `RawModuleDefV8` and convert it into a `ModuleDef`,
/// or return a stream of errors if the definition is invalid.
pub fn validate(def: RawModuleDefV8) -> Result<ModuleDef> {
    // The logic here is slightly odd.
    // Most of our errors will come from the v9 validation code.
    // But, there are some errors that can only happen in v8, e.g. multiple primary keys.
    // So, we collect those in a side buffer, do the v9 validation, and then merge the two streams.
    let mut extra_errors = vec![];

    let v9 = upgrade_module(def, &mut extra_errors);

    // Now defer to the v9 validation.
    let result: Result<ModuleDef> = crate::def::validate::v9::validate(v9);
    ValidationErrors::add_extra_errors(result, extra_errors).map_err(ValidationErrors::sort_deduplicate)
}

/// Upgrade a module, returning a v9 module definition.
/// Most of our validation happens in v9, but there are some errors that can only happen in v8;
/// these are pushed to the secondary stream of errors.
fn upgrade_module(def: RawModuleDefV8, extra_errors: &mut Vec<ValidationError>) -> RawModuleDefV9 {
    let RawModuleDefV8 {
        typespace,
        tables,
        reducers,
        misc_exports,
    } = def;

    let tables = convert_all(tables, |table| upgrade_table(table, &typespace, extra_errors));
    let reducers = convert_all(reducers, upgrade_reducer);
    let types = misc_exports.into_iter().map(upgrade_misc_export_to_type).collect();

    RawModuleDefV9 {
        typespace,
        tables,
        reducers,
        types,
        // V8 module defs don't have procedures or column default values,
        // which are all we use the `misc_exports` for at this time (pgoldman 2025-10-09).
        misc_exports: Default::default(),
        row_level_security: vec![], // v8 doesn't have row-level security
    }
}

/// Get an iterator deriving [RawIndexDefV8]s from the constraints that require them like `UNIQUE`.
///
/// It looks into [Self::constraints] for possible duplicates and remove them from the result
pub fn generated_indexes(table: &RawTableDefV8) -> impl Iterator<Item = RawIndexDefV8> + '_ {
    table
        .constraints
        .iter()
        // We are only interested in constraints implying an index.
        .filter(|x| x.constraints.has_indexed())
        // Create the `IndexDef`.
        .map(|x| {
            let is_unique = x.constraints.has_unique();
            RawIndexDefV8::for_column(&table.table_name, &x.constraint_name, x.columns.clone(), is_unique)
        })
        // Only keep those we don't yet have in the list of indices (checked by name).
        .filter(|idx| table.indexes.iter().all(|x| x.index_name != idx.index_name))
}

/// Get an iterator deriving [RawSequenceDefV8] from the constraints that require them like `IDENTITY`.
///
/// It looks into [Self::constraints] for possible duplicates and remove them from the result
pub fn generated_sequences(table: &RawTableDefV8) -> impl Iterator<Item = RawSequenceDefV8> + '_ {
    let cols: HashSet<_> = table.sequences.iter().map(|seq| ColList::new(seq.col_pos)).collect();

    table
        .constraints
        .iter()
        // We are only interested in constraints implying a sequence.
        .filter(move |x| !cols.contains(&x.columns) && x.constraints.has_autoinc())
        // Create the `SequenceDef`.
        .map(|x| RawSequenceDefV8::for_column(&table.table_name, &x.constraint_name, x.columns.head().unwrap()))
        // Only keep those we don't yet have in the list of sequences (checked by name).
        .filter(|seq| table.sequences.iter().all(|x| x.sequence_name != seq.sequence_name))
}

/// Get an iterator deriving [RawConstraintDefV8] from the indexes that require them like `UNIQUE`.
///
/// It looks into Self::constraints for possible duplicates and remove them from the result
pub fn generated_constraints(table: &RawTableDefV8) -> impl Iterator<Item = RawConstraintDefV8> + '_ {
    // Collect the set of all col-lists with a constraint.
    let cols: HashSet<_> = table
        .constraints
        .iter()
        .filter(|x| x.constraints.kind() != ConstraintKind::UNSET)
        .map(|x| &x.columns)
        .collect();

    // Those indices that are not present in the constraints above
    // have constraints generated for them.
    // When `idx.is_unique`, a unique constraint is generated rather than an indexed one.
    table
        .indexes
        .iter()
        .filter(move |idx: &&RawIndexDefV8| !cols.contains(&idx.columns))
        .map(|idx| table.gen_constraint_def(Constraints::from_is_unique(idx.is_unique), idx.columns.clone()))
}

/// Upgrade a table, returning a v9 table definition and a stream of v8-only validation errors.
fn upgrade_table(
    table: RawTableDescV8,
    typespace: &Typespace,
    extra_errors: &mut Vec<ValidationError>,
) -> RawTableDefV9 {
    // First, generate all the various things that are needed.
    // This is the hairiest part of v8.
    let generated_constraints = generated_constraints(&table.schema).collect::<Vec<_>>();
    let generated_sequences = generated_sequences(&table.schema).collect::<Vec<_>>();
    let generated_indexes = generated_indexes(&table.schema).collect::<Vec<_>>();

    let RawTableDescV8 {
        schema:
            RawTableDefV8 {
                table_name,
                columns,
                indexes,
                constraints,
                sequences,
                table_type,
                table_access,
                scheduled,
            },
        data: product_type_ref,
    } = table;

    // Check all column defs, then discard them.
    let scheduled_at_col = columns
        .iter()
        .position(|x| &*x.col_name == "scheduled_at")
        .map(|i| i as u16);
    check_all_column_defs(product_type_ref, columns, &table_name, typespace, extra_errors);

    // Now we're ready to go through the various definitions and upgrade them.
    let indexes = convert_all(indexes.into_iter().chain(generated_indexes), upgrade_index);
    let sequences = convert_all(sequences.into_iter().chain(generated_sequences), upgrade_sequence);
    let schedule = upgrade_schedule(scheduled, scheduled_at_col);

    // Constraints are pretty hairy, which is why we're getting rid of v8.
    let mut primary_key = None;
    let unique_constraints = constraints
        .into_iter()
        .chain(generated_constraints)
        .filter_map(|constraint| upgrade_constraint(constraint, &table_name, &mut primary_key, extra_errors))
        .collect();

    let table_type = table_type.into();
    let table_access = table_access.into();

    RawTableDefV9 {
        name: table_name,
        product_type_ref,
        primary_key: ColList::from_iter(primary_key),
        indexes,
        constraints: unique_constraints,
        sequences,
        schedule,
        table_type,
        table_access,
    }
}

/// Check all column definitions.
/// This is a v8-only validation step, since v9 has no notion of a column definition, relying solely on the product_type_ref to define columns.
fn check_all_column_defs(
    product_type_ref: AlgebraicTypeRef,
    columns: Vec<RawColumnDefV8>,
    table_name: &RawIdentifier,
    typespace: &Typespace,
    extra_errors: &mut Vec<ValidationError>,
) {
    // Next, check that the ColumnDefs are compatible with the product_type_ref.
    // In v8, sometimes the types in ColumnDefs were resolved.
    // So, we need to resolve everything here before validationg.
    // First, we resolve the product type.
    let resolved_product_type = typespace
        .get(product_type_ref)
        .and_then(AlgebraicType::as_product)
        .map(|product_type| WithTypespace::new(typespace, product_type).resolve_refs());

    match resolved_product_type {
        Some(Ok(resolved_product_type)) => {
            // We've found a useful product type, check the column definitions and discard them.
            for (i, column) in columns.into_iter().enumerate() {
                check_column(
                    i.into(),
                    column,
                    &resolved_product_type,
                    table_name,
                    typespace,
                    extra_errors,
                );
            }
        }
        _ => {
            extra_errors.push(ValidationError::InvalidProductTypeRef {
                table: table_name.clone(),
                ref_: product_type_ref,
            });
        }
    }
}

/// Check a column definition.
fn check_column(
    id: ColId,
    column: RawColumnDefV8,
    resolved_product_type: &ProductType,
    table_name: &RawIdentifier,
    typespace: &Typespace,
    extra_errors: &mut Vec<ValidationError>,
) {
    let RawColumnDefV8 { col_name, col_type } = column;

    // for some reason, the original `RawColumnDefv8` sometimes stored *resolved* types.
    // so, resolve before checking for equality.

    let element = resolved_product_type.elements.get(id.idx());

    let resolved_col_ty = WithTypespace::new(typespace, &col_type).resolve_refs();

    match (element, resolved_col_ty) {
        (Some(element), Ok(resolved_col_ty)) => {
            if !element.has_name(&col_name) || element.algebraic_type != resolved_col_ty {
                extra_errors.push(ValidationError::ColumnDefMalformed {
                    column: RawColumnName::new(table_name.clone(), col_name),
                    ty: resolved_col_ty.into(),
                    pos: id,
                    product_type: resolved_product_type.clone().into(),
                });
            }
        }
        _ => extra_errors.push(ValidationError::ColumnDefMalformed {
            column: RawColumnName::new(table_name.clone(), col_name),
            ty: col_type.into(),
            pos: id,
            product_type: resolved_product_type.clone().into(),
        }),
    }
}

/// Upgrade an index.
fn upgrade_index(index: RawIndexDefV8) -> RawIndexDefV9 {
    let RawIndexDefV8 {
        index_name,
        is_unique: _, // handled by generated_constraints
        index_type,
        columns,
    } = index;

    let algorithm = match index_type {
        IndexType::BTree => RawIndexAlgorithm::BTree { columns },
        IndexType::Hash => RawIndexAlgorithm::Hash { columns },
    };
    // The updated bindings macros will correctly distinguish between accessor name and index name as specified in the
    // ABI stability proposal. The old macros don't make this distinction, so we just reuse the name for them.
    let accessor_name = Some(index_name.clone());
    RawIndexDefV9 {
        name: Some(index_name),
        // Set the accessor name to be the same as the index name.
        accessor_name,
        algorithm,
    }
}

/// Upgrade a constraint.
///
/// `primary_key` is mutable and will be set to `Some(constraint.columns.as_singleton())` if the constraint is a primary key.
/// If it has already been set, an error will be pushed to `extra_errors`.
fn upgrade_constraint(
    constraint: RawConstraintDefV8,
    table_name: &RawIdentifier,
    primary_key: &mut Option<ColId>,
    extra_errors: &mut Vec<ValidationError>,
) -> Option<RawConstraintDefV9> {
    let RawConstraintDefV8 {
        constraint_name, // not used in v9.
        constraints,
        columns,
    } = constraint;

    if constraints.has_primary_key() {
        if let Some(col) = columns.as_singleton() {
            let replaced = primary_key.replace(col);
            if replaced.is_some() {
                extra_errors.push(ValidationError::RepeatedPrimaryKey {
                    table: table_name.clone(),
                });
            }
        } else {
            // There is a primary key annotation on multiple columns.
            // client codegen can't handle this.
            extra_errors.push(ValidationError::RepeatedPrimaryKey {
                table: table_name.clone(),
            });
        }
    }

    if constraints.has_unique() {
        Some(RawConstraintDefV9 {
            name: Some(constraint_name),
            data: RawConstraintDataV9::Unique(RawUniqueConstraintDataV9 { columns }),
        })
    } else {
        // other constraints are implemented by `generated_sequences`.
        // Note that `Constraints::unset` will not trigger any of the preceding branches, so will be ignored.
        // This is consistent with the original `TableSchema::from_(raw_)def`, which also ignored `Constraints::unset`.
        None
    }
}

fn upgrade_schedule(schedule: Option<RawIdentifier>, scheduled_at_col: Option<u16>) -> Option<RawScheduleDefV9> {
    let scheduled_at_col = scheduled_at_col?;
    schedule.map(|reducer_name| RawScheduleDefV9 {
        name: None,
        reducer_name,
        scheduled_at_column: scheduled_at_col.into(),
    })
}

fn upgrade_sequence(sequence: RawSequenceDefV8) -> RawSequenceDefV9 {
    let RawSequenceDefV8 {
        sequence_name,
        col_pos,
        increment,
        start,
        min_value,
        max_value,
        allocated: _, // not used in v9.
    } = sequence;

    RawSequenceDefV9 {
        name: Some(sequence_name),
        column: col_pos,
        start,
        increment,
        min_value,
        max_value,
    }
}

fn upgrade_reducer(reducer: RawReducerDefV8) -> RawReducerDefV9 {
    let RawReducerDefV8 { name, args } = reducer;
    let lifecycle = match &name[..] {
        INIT_NAME => Some(Lifecycle::Init),
        IDENTITY_CONNECTED_NAME => Some(Lifecycle::OnConnect),
        IDENTITY_DISCONNECTED_NAME => Some(Lifecycle::OnDisconnect),
        _ => None,
    };
    RawReducerDefV9 {
        name,
        // v9 uses the correct name :-)
        params: ProductType::from_iter(args),
        lifecycle,
    }
}

/// The only possible `RawMiscModuleExportV8` is a type name.
fn upgrade_misc_export_to_type(misc_export: RawMiscModuleExportV8) -> RawTypeDefV9 {
    let RawMiscModuleExportV8::TypeAlias(RawTypeAliasV8 { name, ty }) = misc_export;

    let name = sats_name_to_scoped_name(&name);

    RawTypeDefV9 {
        name,
        ty,
        // all types have a custom ordering in v8
        custom_ordering: true,
    }
}

fn convert_all<T, U>(input: impl IntoIterator<Item = T>, f: impl FnMut(T) -> U) -> Vec<U> {
    input.into_iter().map(f).collect()
}

#[cfg(test)]
mod tests {
    use crate::def::validate::tests::{check_product_type, expect_identifier, expect_type_name};
    use crate::def::validate::v8::{IDENTITY_CONNECTED_NAME, IDENTITY_DISCONNECTED_NAME, INIT_NAME};
    use crate::def::{validate::Result, ModuleDef};
    use crate::error::*;
    use crate::type_for_generate::ClientCodegenError;

    use spacetimedb_data_structures::expect_error_matching;
    use spacetimedb_lib::db::raw_def::*;
    use spacetimedb_lib::{ScheduleAt, TableDesc};
    use spacetimedb_primitives::{ColId, ColList, Constraints};
    use spacetimedb_sats::{AlgebraicType, AlgebraicTypeRef, ProductType};
    use v8::RawModuleDefV8Builder;
    use v9::Lifecycle;

    /// This test attempts to exercise every successful path in the validation code.
    #[test]
    fn valid_definition() {
        let mut builder = RawModuleDefV8Builder::default();

        let product_type = AlgebraicType::product([("a", AlgebraicType::U64), ("b", AlgebraicType::String)]);
        let product_type_ref = builder.add_type_for_tests("scope1.scope2.ReferencedProduct", product_type.clone());

        let sum_type = AlgebraicType::simple_enum(["Gala", "GrannySmith", "RedDelicious"].into_iter());
        let sum_type_ref = builder.add_type_for_tests("ReferencedSum", sum_type.clone());

        let schedule_at_type = builder.add_type::<ScheduleAt>();

        builder.add_table_for_tests(RawTableDefV8::new_for_tests(
            "Apples",
            ProductType::from([
                ("id", AlgebraicType::U64),
                ("name", AlgebraicType::String),
                ("count", AlgebraicType::U16),
                ("type", sum_type_ref.into()),
            ]),
        ));

        builder.add_table_for_tests(
            RawTableDefV8::new_for_tests(
                "Bananas",
                ProductType::from([
                    ("count", AlgebraicType::U16),
                    ("id", AlgebraicType::U64),
                    ("name", AlgebraicType::String),
                    (
                        "optional_product_column",
                        AlgebraicType::option(product_type_ref.into()),
                    ),
                ]),
            )
            .with_column_constraint(Constraints::primary_key_auto(), 0)
            .with_column_index([0, 1, 2], false),
        );

        let deliveries_product_type = builder.add_table_for_tests(
            RawTableDefV8::new_for_tests(
                "Deliveries",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("scheduled_at", schedule_at_type.clone()),
                    ("scheduled_id", AlgebraicType::U64),
                ]),
            )
            .with_column_constraint(Constraints::primary_key_auto(), 2)
            .with_scheduled(Some("check_deliveries".into())),
        );

        builder.add_reducer_for_tests(INIT_NAME, ProductType::unit());
        builder.add_reducer_for_tests(IDENTITY_CONNECTED_NAME, ProductType::unit());
        builder.add_reducer_for_tests(IDENTITY_DISCONNECTED_NAME, ProductType::unit());
        builder.add_reducer_for_tests(
            "check_deliveries",
            ProductType::from([("a", deliveries_product_type.into())]),
        );
        builder.add_reducer_for_tests("extra_reducer", ProductType::from([("a", AlgebraicType::U64)]));

        let def: ModuleDef = builder.finish().try_into().unwrap();

        let apples = expect_identifier("Apples");
        let bananas = expect_identifier("Bananas");
        let deliveries = expect_identifier("Deliveries");

        assert_eq!(def.tables.len(), 3);

        let apples_def = &def.tables[&apples];

        assert_eq!(&apples_def.name, &apples);
        assert_eq!(apples_def.columns.len(), 4);
        assert_eq!(apples_def.columns[0].name, expect_identifier("id"));
        assert_eq!(apples_def.columns[0].ty, AlgebraicType::U64);
        assert_eq!(apples_def.columns[1].name, expect_identifier("name"));
        assert_eq!(apples_def.columns[1].ty, AlgebraicType::String);
        assert_eq!(apples_def.columns[2].name, expect_identifier("count"));
        assert_eq!(apples_def.columns[2].ty, AlgebraicType::U16);
        assert_eq!(apples_def.columns[3].name, expect_identifier("type"));
        assert_eq!(apples_def.columns[3].ty, sum_type_ref.into());
        assert_eq!(apples_def.primary_key, None);

        let bananas_def = &def.tables[&bananas];

        assert_eq!(&bananas_def.name, &bananas);
        assert_eq!(bananas_def.columns.len(), 4);
        assert_eq!(bananas_def.columns[0].name, expect_identifier("count"));
        assert_eq!(bananas_def.columns[0].ty, AlgebraicType::U16);
        assert_eq!(bananas_def.columns[1].name, expect_identifier("id"));
        assert_eq!(bananas_def.columns[1].ty, AlgebraicType::U64);
        assert_eq!(bananas_def.columns[2].name, expect_identifier("name"));
        assert_eq!(bananas_def.columns[2].ty, AlgebraicType::String);
        assert_eq!(
            bananas_def.columns[3].name,
            expect_identifier("optional_product_column")
        );
        assert_eq!(
            bananas_def.columns[3].ty,
            AlgebraicType::option(product_type_ref.into())
        );
        assert_eq!(bananas_def.primary_key, Some(0.into()));

        let delivery_def = &def.tables[&deliveries];
        assert_eq!(&delivery_def.name, &deliveries);
        assert_eq!(delivery_def.columns.len(), 3);
        assert_eq!(delivery_def.columns[0].name, expect_identifier("id"));
        assert_eq!(delivery_def.columns[0].ty, AlgebraicType::U64);
        assert_eq!(delivery_def.columns[1].name, expect_identifier("scheduled_at"));
        assert_eq!(delivery_def.columns[1].ty, schedule_at_type);
        assert_eq!(delivery_def.columns[2].name, expect_identifier("scheduled_id"));
        assert_eq!(delivery_def.columns[2].ty, AlgebraicType::U64);
        assert_eq!(delivery_def.schedule.as_ref().unwrap().at_column, 1.into());
        assert_eq!(
            &delivery_def.schedule.as_ref().unwrap().function_name[..],
            "check_deliveries"
        );
        assert_eq!(delivery_def.primary_key, Some(ColId(2)));

        assert_eq!(def.typespace.get(product_type_ref), Some(&product_type));
        assert_eq!(def.typespace.get(sum_type_ref), Some(&sum_type));

        check_product_type(&def, apples_def);
        check_product_type(&def, bananas_def);
        check_product_type(&def, delivery_def);

        let product_type_name = expect_type_name("scope1::scope2::ReferencedProduct");
        let sum_type_name = expect_type_name("ReferencedSum");
        let apples_type_name = expect_type_name("Apples");
        let bananas_type_name = expect_type_name("Bananas");
        let deliveries_type_name = expect_type_name("Deliveries");

        assert_eq!(def.types[&product_type_name].ty, product_type_ref);
        assert_eq!(def.types[&sum_type_name].ty, sum_type_ref);
        assert_eq!(def.types[&apples_type_name].ty, apples_def.product_type_ref);
        assert_eq!(def.types[&bananas_type_name].ty, bananas_def.product_type_ref);
        assert_eq!(def.types[&deliveries_type_name].ty, delivery_def.product_type_ref);

        let init_name = expect_identifier(INIT_NAME);
        assert_eq!(def.reducers[&init_name].name, init_name);
        assert_eq!(def.reducers[&init_name].lifecycle, Some(Lifecycle::Init));

        let identity_connected_name = expect_identifier(IDENTITY_CONNECTED_NAME);
        assert_eq!(def.reducers[&identity_connected_name].name, identity_connected_name);
        assert_eq!(
            def.reducers[&identity_connected_name].lifecycle,
            Some(Lifecycle::OnConnect)
        );

        let identity_disconnected_name = expect_identifier(IDENTITY_DISCONNECTED_NAME);
        assert_eq!(
            def.reducers[&identity_disconnected_name].name,
            identity_disconnected_name
        );
        assert_eq!(
            def.reducers[&identity_disconnected_name].lifecycle,
            Some(Lifecycle::OnDisconnect)
        );

        let extra_reducer_name = expect_identifier("extra_reducer");
        assert_eq!(def.reducers[&extra_reducer_name].name, extra_reducer_name);
        assert_eq!(def.reducers[&extra_reducer_name].lifecycle, None);
        assert_eq!(
            def.reducers[&extra_reducer_name].params,
            ProductType::from([("a", AlgebraicType::U64)])
        );

        let check_deliveries_name = expect_identifier("check_deliveries");
        assert_eq!(def.reducers[&check_deliveries_name].name, check_deliveries_name);
        assert_eq!(def.reducers[&check_deliveries_name].lifecycle, None);
        assert_eq!(
            def.reducers[&check_deliveries_name].params,
            ProductType::from([("a", deliveries_product_type.into())])
        );
    }

    #[test]
    fn invalid_product_type_ref() {
        let mut builder = RawModuleDefV8Builder::default();

        // `add_table` does NOT initialize table.product_type_ref, which should result in an error.
        builder.add_table(TableDesc {
            schema: RawTableDefV8::new_for_tests("Bananas", ProductType::from([("count", AlgebraicType::U32)])),
            data: AlgebraicTypeRef(1337),
        });

        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::InvalidProductTypeRef { table, ref_ } => {
            &table[..] == "Bananas" && ref_ == &AlgebraicTypeRef(1337)
        });
    }

    #[test]
    fn invalid_table_name() {
        let mut builder = RawModuleDefV8Builder::default();
        builder.add_table_for_tests(RawTableDefV8::new_for_tests(
            "",
            ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
        ));
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::IdentifierError { error } => {
            error == &IdentifierError::Empty {}
        });
    }

    #[test]
    fn invalid_column_name() {
        let mut builder = RawModuleDefV8Builder::default();
        builder.add_table_for_tests(RawTableDefV8::new_for_tests(
            "",
            ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
        ));
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::IdentifierError { error } => {
            error == &IdentifierError::Empty {}
        });
    }

    #[test]
    fn invalid_index_column_ref() {
        let mut builder = RawModuleDefV8Builder::default();
        builder.add_table_for_tests(
            RawTableDefV8::new_for_tests(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
            )
            .with_column_index(ColList::from_iter([0, 55]), false),
        );
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ColumnNotFound { table, column, .. } => {
            &table[..] == "Bananas" &&
            column == &55.into()
        });
    }

    #[test]
    fn invalid_unique_constraint_column_ref() {
        let mut builder = RawModuleDefV8Builder::default();
        builder.add_table_for_tests(
            RawTableDefV8::new_for_tests(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
            )
            .with_column_constraint(Constraints::unique(), ColList::from_iter([55])),
        );
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ColumnNotFound { table, column, .. } => {
            &table[..] == "Bananas" &&
            column == &55.into()
        });
    }

    #[test]
    fn invalid_sequence_column_ref() {
        // invalid column id
        let mut builder = RawModuleDefV8Builder::default();
        builder.add_table_for_tests(
            RawTableDefV8::new_for_tests(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
            )
            .with_column_sequence(ColId(55)),
        );
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ColumnNotFound { table, column, .. } => {
            &table[..] == "Bananas" &&
            column == &55.into()
        });

        // incorrect column type
        let mut builder = RawModuleDefV8Builder::default();
        builder.add_table_for_tests(
            RawTableDefV8::new_for_tests(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::String)]),
            )
            .with_column_sequence(ColId(1)),
        );
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::InvalidSequenceColumnType { column, column_type, .. } => {
            column == &RawColumnName::new("Bananas", "a") &&
            column_type.0 == AlgebraicType::String
        });
    }

    #[test]
    fn invalid_index_column_duplicates() {
        let mut builder = RawModuleDefV8Builder::default();
        builder.add_table_for_tests(
            RawTableDefV8::new_for_tests(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
            )
            .with_column_index(ColList::from_iter([0, 0]), false),
        );
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateColumns{ columns, .. } => {
            columns == &ColList::from_iter([0, 0])
        });
    }

    #[test]
    fn invalid_unique_constraint_column_duplicates() {
        let mut builder = RawModuleDefV8Builder::default();
        builder.add_table_for_tests(
            RawTableDefV8::new_for_tests(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
            )
            .with_column_constraint(Constraints::unique(), ColList::from_iter([1, 1])),
        );
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateColumns{ columns, .. } => {
            columns == &ColList::from_iter([1, 1])
        });
    }

    #[test]
    fn recursive_ref() {
        let recursive_type = AlgebraicType::product([("a", AlgebraicTypeRef(0).into())]);

        let mut builder = RawModuleDefV8Builder::default();
        let ref_ = builder.add_type_for_tests("Recursive", recursive_type.clone());
        builder.add_reducer_for_tests("silly", ProductType::from([("a", ref_.into())]));
        let result: ModuleDef = builder.finish().try_into().unwrap();

        assert!(result.typespace_for_generate[ref_].is_recursive());
    }

    #[test]
    fn out_of_bounds_ref() {
        let invalid_type_1 = AlgebraicType::product([("a", AlgebraicTypeRef(31).into())]);
        let mut builder = RawModuleDefV8Builder::default();
        let ref_ = builder.add_type_for_tests("Invalid", invalid_type_1.clone());
        builder.add_reducer_for_tests("silly", ProductType::from([("a", ref_.into())]));
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ClientCodegenError { location, error: ClientCodegenError::TypeRefError(_)  } => {
            location == &TypeLocation::InTypespace { ref_: AlgebraicTypeRef(0) }
        });
    }

    #[test]
    fn invalid_use() {
        let inner_type_invalid_for_use = AlgebraicType::product([("b", AlgebraicType::U32)]);
        let invalid_type = AlgebraicType::product([("a", inner_type_invalid_for_use.clone())]);
        let mut builder = RawModuleDefV8Builder::default();
        let ref_ = builder.add_type_for_tests("Invalid", invalid_type.clone());
        builder.add_reducer_for_tests("silly", ProductType::from([("a", ref_.into())]));
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(
            result,
            ValidationError::ClientCodegenError {
                location,
                error: ClientCodegenError::NonSpecialTypeNotAUse { ty }
            } => {
                location == &TypeLocation::InTypespace { ref_: AlgebraicTypeRef(0) } &&
                ty.0 == inner_type_invalid_for_use
            }
        );
    }

    #[test]
    fn only_btree_indexes() {
        let mut builder = RawModuleDefV8Builder::default();
        builder.add_table_for_tests(
            RawTableDefV8::new_for_tests(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
            )
            .with_indexes(vec![RawIndexDefV8 {
                columns: ColList::from_iter([0]),
                is_unique: false,
                index_name: "Bananas_index".into(),
                index_type: IndexType::Hash,
            }]),
        );
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::HashIndexUnsupported { index } => {
            &index[..] == "Bananas_index"
        });
    }

    #[test]
    fn invalid_primary_key() {
        let mut builder = RawModuleDefV8Builder::default();
        builder.add_table_for_tests(
            RawTableDefV8::new_for_tests(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
            )
            .with_column_constraint(Constraints::primary_key(), ColList::from_iter([44])),
        );
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ColumnNotFound { table, column, .. } => {
            &table[..] == "Bananas" &&
            column == &44.into()
        });
    }

    #[test]
    fn duplicate_type_name() {
        let mut builder = RawModuleDefV8Builder::default();
        builder.add_type_for_tests("scope1.scope2.Duplicate", AlgebraicType::U64);
        builder.add_type_for_tests("scope1::scope2::Duplicate", AlgebraicType::U32);
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateTypeName { name } => {
            name == &expect_type_name("scope1::scope2::Duplicate")
        });
    }
}
