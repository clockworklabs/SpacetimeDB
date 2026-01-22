use std::borrow::Cow;

use spacetimedb_data_structures::map::HashCollectionExt;
use spacetimedb_lib::bsatn::Deserializer;
use spacetimedb_lib::db::raw_def::v10::*;
use spacetimedb_lib::de::DeserializeSeed as _;
use spacetimedb_sats::{Typespace, WithTypespace};

use crate::def::validate::v9::{
    check_function_names_are_unique, check_scheduled_functions_exist, generate_schedule_name, identifier,
    CoreValidator, TableValidator, ViewValidator,
};
use crate::def::*;
use crate::error::ValidationError;
use crate::type_for_generate::ProductTypeDef;
use crate::{def::validate::Result, error::TypeLocation};

/// Validate a `RawModuleDefV9` and convert it into a `ModuleDef`,
/// or return a stream of errors if the definition is invalid.
pub fn validate(def: RawModuleDefV10) -> Result<ModuleDef> {
    let typespace = def.typespace().cloned().unwrap_or_else(|| Typespace::EMPTY.clone());
    let known_type_definitions = def.types().into_iter().flatten().map(|def| def.ty);

    let mut validator = ModuleValidatorV10 {
        core: CoreValidator {
            typespace: &typespace,
            stored_in_table_def: Default::default(),
            type_namespace: Default::default(),
            lifecycle_reducers: Default::default(),
            typespace_for_generate: TypespaceForGenerate::builder(&typespace, known_type_definitions),
        },
    };

    // Important general note:
    // This file uses the `ErrorStream` combinator to return *multiple errors
    // at once* when validating a definition.
    // The general pattern is that we use `collect_all_errors` when building
    // a collection, and `combine_errors` when we have multiple
    // things to validate that are independent of each other.
    // We try to avoid using `?` until the end of a function, after we've called
    // `combine_errors` or `collect_all_errors` on all the things we need to validate.
    // Sometimes it is unavoidable to use `?` early and this should be commented on.

    let reducers = def
        .reducers()
        .cloned()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(_idx, reducer)| {
            validator
                .validate_reducer_def(reducer)
                .map(|reducer_def| (reducer_def.name.clone(), reducer_def))
        })
        // Collect into a `Vec` first to preserve duplicate names.
        // Later on, in `check_function_names_are_unique`, we'll transform this into an `IndexMap`.
        .collect_all_errors::<Vec<_>>();

    let procedures = def
        .procedures()
        .cloned()
        .into_iter()
        .flatten()
        .map(|procedure| {
            validator
                .validate_procedure_def(procedure)
                .map(|procedure_def| (procedure_def.name.clone(), procedure_def))
        })
        // Collect into a `Vec` first to preserve duplicate names.
        // Later on, in `check_function_names_are_unique`, we'll transform this into an `IndexMap`.
        .collect_all_errors::<Vec<_>>();

    let views = def
        .views()
        .cloned()
        .into_iter()
        .flatten()
        .map(|view| {
            validator
                .validate_view_def(view)
                .map(|view_def| (view_def.name.clone(), view_def))
        })
        .collect_all_errors();

    let tables = def
        .tables()
        .cloned()
        .into_iter()
        .flatten()
        .map(|table| {
            validator
                .validate_table_def(table)
                .map(|table_def| (table_def.name.clone(), table_def))
        })
        .collect_all_errors();

    let mut refmap = HashMap::default();
    let types = def
        .types()
        .cloned()
        .into_iter()
        .flatten()
        .map(|ty| {
            validator.core.validate_type_def(ty).map(|type_def| {
                refmap.insert(type_def.ty, type_def.name.clone());
                (type_def.name.clone(), type_def)
            })
        })
        .collect_all_errors::<HashMap<_, _>>();

    // Validate schedules - they need the validated tables to exist first
    let schedules = tables
        .as_ref()
        .ok()
        .map(|tables_map| {
            def.schedules()
                .cloned()
                .into_iter()
                .flatten()
                .map(|schedule| validator.validate_schedule_def(schedule, tables_map))
                .collect_all_errors::<Vec<_>>()
        })
        .unwrap_or_else(|| Ok(Vec::new()));

    // Validate lifecycle reducers - they reference reducers by name
    let lifecycle_validations = reducers
        .as_ref()
        .ok()
        .map(|reducers_vec| {
            def.lifecycle_reducers()
                .cloned()
                .into_iter()
                .flatten()
                .map(|lifecycle_def| {
                    let function_name = identifier(lifecycle_def.function_name.clone())?;

                    let (pos, _) = reducers_vec
                        .iter()
                        .enumerate()
                        .find(|(_, (_, r))| r.name == function_name)
                        .ok_or_else(|| ValidationError::LifecycleWithoutReducer {
                            lifecycle: lifecycle_def.lifecycle_spec,
                        })?;

                    let reducer_id = ReducerId(pos as u32);

                    validator.validate_lifecycle_reducer(lifecycle_def.clone(), reducer_id)?;

                    Ok((reducer_id, lifecycle_def.lifecycle_spec))
                })
                .collect_all_errors::<Vec<_>>()
        })
        .unwrap_or_else(|| Ok(Vec::new()));
    // Combine all validation results
    let tables_types_reducers_procedures_views = (
        tables,
        types,
        reducers,
        procedures,
        views,
        schedules,
        lifecycle_validations,
    )
        .combine_errors()
        .and_then(
            |(mut tables, types, reducers, procedures, views, schedules, lifecycles)| {
                let (mut reducers, procedures, views) = check_function_names_are_unique(reducers, procedures, views)?;

                // Attach lifecycles to their respective reducers
                attach_lifecycles_to_reducers(&mut reducers, lifecycles)?;

                // Attach schedules to their respective tables
                attach_schedules_to_tables(&mut tables, schedules)?;

                check_scheduled_functions_exist(&mut tables, &reducers, &procedures)?;

                Ok((tables, types, reducers, procedures, views))
            },
        );

    let CoreValidator {
        stored_in_table_def,
        typespace_for_generate,
        lifecycle_reducers,
        ..
    } = validator.core;

    let (tables, types, reducers, procedures, views) =
        (tables_types_reducers_procedures_views).map_err(|errors| errors.sort_deduplicate())?;

    let typespace_for_generate = typespace_for_generate.finish();

    Ok(ModuleDef {
        tables,
        reducers,
        views,
        types,
        typespace,
        typespace_for_generate,
        stored_in_table_def,
        refmap,
        row_level_security_raw: HashMap::new(),
        lifecycle_reducers,
        procedures,
        raw_module_def_version: RawModuleDefVersion::V10,
    })
}

struct ModuleValidatorV10<'a> {
    core: CoreValidator<'a>,
}

impl<'a> ModuleValidatorV10<'a> {
    fn validate_table_def(&mut self, table: RawTableDefV10) -> Result<TableDef> {
        let RawTableDefV10 {
            name: raw_table_name,
            product_type_ref,
            primary_key,
            indexes,
            constraints,
            sequences,
            table_type,
            table_access,
            default_values,
        } = table;

        let product_type: &ProductType = self
            .core
            .typespace
            .get(product_type_ref)
            .and_then(AlgebraicType::as_product)
            .ok_or_else(|| {
                ValidationErrors::from(ValidationError::InvalidProductTypeRef {
                    table: raw_table_name.clone(),
                    ref_: product_type_ref,
                })
            })?;

        let mut table_validator =
            TableValidator::new(raw_table_name.clone(), product_type_ref, product_type, &mut self.core);

        // Validate columns first
        let mut columns: Vec<ColumnDef> = (0..product_type.elements.len())
            .map(|id| table_validator.validate_column_def(id.into()))
            .collect_all_errors()?;

        let indexes = indexes
            .into_iter()
            .map(|index| {
                table_validator
                    .validate_index_def(index)
                    .map(|index| (index.name.clone(), index))
            })
            .collect_all_errors::<StrMap<_>>();

        let constraints_primary_key = constraints
            .into_iter()
            .map(|constraint| {
                table_validator
                    .validate_constraint_def(constraint)
                    .map(|constraint| (constraint.name.clone(), constraint))
            })
            .collect_all_errors()
            .and_then(|constraints: StrMap<ConstraintDef>| {
                table_validator.validate_primary_key(constraints, primary_key)
            });

        let constraints_backed_by_indices =
            if let (Ok((constraints, _)), Ok(indexes)) = (&constraints_primary_key, &indexes) {
                constraints
                    .values()
                    .filter_map(|c| c.data.unique_columns().map(|cols| (c, cols)))
                    .filter(|(_, unique_cols)| {
                        !indexes
                            .values()
                            .any(|i| ColSet::from(i.algorithm.columns()) == **unique_cols)
                    })
                    .map(|(c, cols)| {
                        let constraint = c.name.clone();
                        let columns = cols.clone();
                        Err(ValidationError::UniqueConstraintWithoutIndex { constraint, columns }.into())
                    })
                    .collect_all_errors()
            } else {
                Ok(())
            };

        let sequences = sequences
            .into_iter()
            .map(|sequence| {
                table_validator
                    .validate_sequence_def(sequence)
                    .map(|sequence| (sequence.name.clone(), sequence))
            })
            .collect_all_errors();

        let name = table_validator
            .add_to_global_namespace(raw_table_name.clone())
            .and_then(|name| {
                let name = identifier(name)?;
                if table_type != TableType::System && name.starts_with("st_") {
                    Err(ValidationError::TableNameReserved { table: name }.into())
                } else {
                    Ok(name)
                }
            });

        // Validate default values inline and attach them to columns
        let validated_defaults: Result<HashMap<ColId, AlgebraicValue>> = default_values
            .iter()
            .map(|cdv| {
                let col_id = cdv.col_id;
                let Some(col_elem) = product_type.elements.get(col_id.idx()) else {
                    return Err(ValidationError::ColumnNotFound {
                        table: raw_table_name.clone(),
                        def: raw_table_name.clone(),
                        column: col_id,
                    }
                    .into());
                };

                let mut reader = &cdv.value[..];
                let ty = WithTypespace::new(self.core.typespace, &col_elem.algebraic_type);
                let field_value = ty.deserialize(Deserializer::new(&mut reader)).map_err(|decode_error| {
                    ValidationError::ColumnDefaultValueMalformed {
                        table: raw_table_name.clone(),
                        col_id,
                        err: decode_error,
                    }
                })?;

                Ok((col_id, field_value))
            })
            .collect_all_errors();

        let validated_defaults = validated_defaults?;
        // Attach default values to columns
        for column in &mut columns {
            if let Some(default_value) = validated_defaults.get(&column.col_id) {
                column.default_value = Some(default_value.clone());
            }
        }

        let (name, indexes, (constraints, primary_key), (), sequences) = (
            name,
            indexes,
            constraints_primary_key,
            constraints_backed_by_indices,
            sequences,
        )
            .combine_errors()?;

        Ok(TableDef {
            name,
            product_type_ref,
            primary_key,
            columns,
            indexes,
            constraints,
            sequences,
            schedule: None, // V10 handles schedules separately
            table_type,
            table_access,
        })
    }

    fn validate_reducer_def(&mut self, reducer_def: RawReducerDefV10) -> Result<ReducerDef> {
        let RawReducerDefV10 { name, params, .. } = reducer_def;

        let params_for_generate =
            self.core
                .params_for_generate(&params, |position, arg_name| TypeLocation::ReducerArg {
                    reducer_name: (&*name).into(),
                    position,
                    arg_name,
                });

        let name_result = identifier(name);

        let (name_result, params_for_generate) = (name_result, params_for_generate).combine_errors()?;

        Ok(ReducerDef {
            name: name_result,
            params: params.clone(),
            params_for_generate: ProductTypeDef {
                elements: params_for_generate,
                recursive: false, // A ProductTypeDef not stored in a Typespace cannot be recursive.
            },
            lifecycle: None, // V10 handles lifecycle separately
        })
    }

    fn validate_schedule_def(
        &mut self,
        schedule: RawScheduleDefV10,
        tables: &HashMap<Identifier, TableDef>,
    ) -> Result<(ScheduleDef, Box<str>)> {
        let RawScheduleDefV10 {
            name,
            table_name,
            schedule_at_col,
            function_name,
        } = schedule;

        let table_ident = identifier(table_name.clone())?;

        // Look up the table to validate the schedule
        let table = tables.get(&table_ident).ok_or_else(|| ValidationError::TableNotFound {
            table: table_name.clone(),
        })?;

        let product_type = self
            .core
            .typespace
            .get(table.product_type_ref)
            .and_then(AlgebraicType::as_product)
            .ok_or_else(|| ValidationError::InvalidProductTypeRef {
                table: table_name.clone(),
                ref_: table.product_type_ref,
            })?;

        let name = name.unwrap_or_else(|| generate_schedule_name(&table_name));
        self.core
            .validate_schedule_def(
                table_name.clone(),
                identifier(name)?,
                function_name,
                product_type,
                schedule_at_col,
                table.primary_key,
            )
            .map(|schedule_def| (schedule_def, table_name))
    }

    fn validate_lifecycle_reducer(
        &mut self,
        lifecycle_def: RawLifeCycleReducerDefV10,
        reducer_id: ReducerId,
    ) -> Result<Lifecycle> {
        let RawLifeCycleReducerDefV10 {
            lifecycle_spec,
            function_name: _,
        } = lifecycle_def;

        self.core.register_lifecycle(lifecycle_spec, reducer_id)?;
        Ok(lifecycle_spec)
    }

    fn validate_procedure_def(&mut self, procedure_def: RawProcedureDefV10) -> Result<ProcedureDef> {
        let RawProcedureDefV10 {
            name,
            params,
            return_type,
            ..
        } = procedure_def;

        let params_for_generate =
            self.core
                .params_for_generate(&params, |position, arg_name| TypeLocation::ProcedureArg {
                    procedure_name: Cow::Borrowed(&name),
                    position,
                    arg_name,
                });

        let return_type_for_generate = self.core.validate_for_type_use(
            &TypeLocation::ProcedureReturn {
                procedure_name: Cow::Borrowed(&name),
            },
            &return_type,
        );

        let name_result = identifier(name);

        let (name_result, params_for_generate, return_type_for_generate) =
            (name_result, params_for_generate, return_type_for_generate).combine_errors()?;

        Ok(ProcedureDef {
            name: name_result,
            params,
            params_for_generate: ProductTypeDef {
                elements: params_for_generate,
                recursive: false,
            },
            return_type,
            return_type_for_generate,
        })
    }

    fn validate_view_def(&mut self, view_def: RawViewDefV10) -> Result<ViewDef> {
        let RawViewDefV10 {
            name,
            is_public,
            is_anonymous,
            params,
            return_type,
            index,
        } = view_def;

        let invalid_return_type = || {
            ValidationErrors::from(ValidationError::InvalidViewReturnType {
                view: name.clone(),
                ty: return_type.clone().into(),
            })
        };

        let product_type_ref = return_type
            .as_option()
            .and_then(AlgebraicType::as_ref)
            .or_else(|| {
                return_type
                    .as_array()
                    .map(|array_type| array_type.elem_ty.as_ref())
                    .and_then(AlgebraicType::as_ref)
            })
            .cloned()
            .ok_or_else(invalid_return_type)?;

        let product_type = self
            .core
            .typespace
            .get(product_type_ref)
            .and_then(AlgebraicType::as_product)
            .ok_or_else(|| {
                ValidationErrors::from(ValidationError::InvalidProductTypeRef {
                    table: name.clone(),
                    ref_: product_type_ref,
                })
            })?;

        let params_for_generate =
            self.core
                .params_for_generate(&params, |position, arg_name| TypeLocation::ViewArg {
                    view_name: Cow::Borrowed(&name),
                    position,
                    arg_name,
                })?;

        let return_type_for_generate = self.core.validate_for_type_use(
            &TypeLocation::ViewReturn {
                view_name: Cow::Borrowed(&name),
            },
            &return_type,
        );

        let mut view_validator = ViewValidator::new(
            name.clone(),
            product_type_ref,
            product_type,
            &params,
            &params_for_generate,
            &mut self.core,
        );

        let name_result = view_validator.add_to_global_namespace(name).and_then(identifier);

        let n = product_type.elements.len();
        let return_columns = (0..n)
            .map(|id| view_validator.validate_view_column_def(id.into()))
            .collect_all_errors();

        let n = params.elements.len();
        let param_columns = (0..n)
            .map(|id| view_validator.validate_param_column_def(id.into()))
            .collect_all_errors();

        let (name_result, return_type_for_generate, return_columns, param_columns) =
            (name_result, return_type_for_generate, return_columns, param_columns).combine_errors()?;

        Ok(ViewDef {
            name: name_result,
            is_anonymous,
            is_public,
            params,
            fn_ptr: index.into(),
            params_for_generate: ProductTypeDef {
                elements: params_for_generate,
                recursive: false, // A `ProductTypeDef` not stored in a `Typespace` cannot be recursive.
            },
            return_type,
            return_type_for_generate,
            product_type_ref,
            return_columns,
            param_columns,
        })
    }
}

fn attach_lifecycles_to_reducers(
    reducers: &mut IndexMap<Identifier, ReducerDef>,
    lifecycles: Vec<(ReducerId, Lifecycle)>,
) -> Result<()> {
    for lifecycle in lifecycles {
        let (reducer_id, lifecycle) = lifecycle;
        let reducer = reducers
            .values_mut()
            .nth(reducer_id.idx())
            .ok_or_else(|| ValidationError::LifecycleWithoutReducer { lifecycle })?;

        // Enforce invariant: only one lifecycle per reducer
        if reducer.lifecycle.is_some() {
            return Err(ValidationError::DuplicateLifecycle { lifecycle }.into());
        }

        reducer.lifecycle = Some(lifecycle);
    }

    Ok(())
}

fn attach_schedules_to_tables(
    tables: &mut HashMap<Identifier, TableDef>,
    schedules: Vec<(ScheduleDef, Box<str>)>,
) -> Result<()> {
    for schedule in schedules {
        let (schedule, table_name) = schedule;
        let table = tables.values_mut().find(|t| *t.name == *table_name).ok_or_else(|| {
            ValidationError::MissingScheduleTable {
                table_name: table_name.clone(),
                schedule_name: schedule.name.clone(),
            }
        })?;

        // Enforce invariant: only one schedule per table
        if table.schedule.is_some() {
            return Err(ValidationError::DuplicateSchedule {
                table: table.name.clone(),
            }
            .into());
        }

        table.schedule = Some(schedule);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::def::validate::tests::{
        check_product_type, expect_identifier, expect_raw_type_name, expect_resolve, expect_type_name,
    };
    use crate::def::{validate::Result, ModuleDef};
    use crate::def::{
        BTreeAlgorithm, ConstraintData, ConstraintDef, DirectAlgorithm, FunctionKind, IndexAlgorithm, IndexDef,
        SequenceDef, UniqueConstraintData,
    };
    use crate::error::*;
    use crate::type_for_generate::ClientCodegenError;

    use itertools::Itertools;
    use spacetimedb_data_structures::expect_error_matching;
    use spacetimedb_lib::db::raw_def::v10::RawModuleDefV10Builder;
    use spacetimedb_lib::db::raw_def::v9::{btree, direct, hash};
    use spacetimedb_lib::db::raw_def::*;
    use spacetimedb_lib::ScheduleAt;
    use spacetimedb_primitives::{ColId, ColList, ColSet};
    use spacetimedb_sats::{AlgebraicType, AlgebraicTypeRef, AlgebraicValue, ProductType, SumValue};
    use v9::{Lifecycle, TableAccess, TableType};

    /// This test attempts to exercise every successful path in the validation code.
    #[test]
    fn valid_definition() {
        let mut builder = RawModuleDefV10Builder::new();

        let product_type = AlgebraicType::product([("a", AlgebraicType::U64), ("b", AlgebraicType::String)]);
        let product_type_ref = builder.add_algebraic_type(
            ["scope1".into(), "scope2".into()],
            "ReferencedProduct",
            product_type.clone(),
            false,
        );

        let sum_type = AlgebraicType::simple_enum(["Gala", "GrannySmith", "RedDelicious"].into_iter());
        let sum_type_ref = builder.add_algebraic_type([], "ReferencedSum", sum_type.clone(), false);

        let schedule_at_type = builder.add_type::<ScheduleAt>();

        let red_delicious = AlgebraicValue::Sum(SumValue::new(2, ()));

        builder
            .build_table_with_new_type(
                "Apples",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("name", AlgebraicType::String),
                    ("count", AlgebraicType::U16),
                    ("type", sum_type_ref.into()),
                ]),
                true,
            )
            .with_index(btree([1, 2]), "apples_id")
            .with_index(direct(2), "Apples_count_direct")
            .with_unique_constraint(2)
            .with_index(btree(3), "Apples_type_btree")
            .with_unique_constraint(3)
            .with_default_column_value(2, AlgebraicValue::U16(37))
            .with_default_column_value(3, red_delicious.clone())
            .finish();

        builder
            .build_table_with_new_type(
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
                false,
            )
            .with_column_sequence(0)
            .with_unique_constraint(ColId(0))
            .with_primary_key(0)
            .with_access(TableAccess::Private)
            .with_index(btree(0), "bananas_count")
            .with_index(btree([0, 1, 2]), "bananas_count_id_name")
            .finish();

        let deliveries_product_type = builder
            .build_table_with_new_type(
                "Deliveries",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("scheduled_at", schedule_at_type.clone()),
                    ("scheduled_id", AlgebraicType::U64),
                ]),
                true,
            )
            .with_auto_inc_primary_key(2)
            .with_index(btree(2), "scheduled_id_index")
            .with_type(TableType::System)
            .finish();

        builder.add_lifecycle_reducer(Lifecycle::Init, "init", ProductType::unit());
        builder.add_lifecycle_reducer(Lifecycle::OnConnect, "on_connect", ProductType::unit());
        builder.add_lifecycle_reducer(Lifecycle::OnDisconnect, "on_disconnect", ProductType::unit());
        builder.add_reducer("extra_reducer", ProductType::from([("a", AlgebraicType::U64)]));
        builder.add_reducer(
            "check_deliveries",
            ProductType::from([("a", deliveries_product_type.into())]),
        );
        builder.add_schedule("Deliveries", 1, "check_deliveries");

        let def: ModuleDef = builder.finish().try_into().unwrap();

        let apples = expect_identifier("Apples");
        let bananas = expect_identifier("Bananas");
        let deliveries = expect_identifier("Deliveries");

        assert_eq!(def.tables.len(), 3);

        let apples_def = &def.tables[&apples];

        assert_eq!(apples_def.name, apples);
        assert_eq!(apples_def.table_type, TableType::User);
        assert_eq!(apples_def.table_access, TableAccess::Public);

        assert_eq!(apples_def.columns.len(), 4);
        assert_eq!(apples_def.columns[0].name, expect_identifier("id"));
        assert_eq!(apples_def.columns[0].ty, AlgebraicType::U64);
        assert_eq!(apples_def.columns[0].default_value, None);
        assert_eq!(apples_def.columns[1].name, expect_identifier("name"));
        assert_eq!(apples_def.columns[1].ty, AlgebraicType::String);
        assert_eq!(apples_def.columns[1].default_value, None);
        assert_eq!(apples_def.columns[2].name, expect_identifier("count"));
        assert_eq!(apples_def.columns[2].ty, AlgebraicType::U16);
        assert_eq!(apples_def.columns[2].default_value, Some(AlgebraicValue::U16(37)));
        assert_eq!(apples_def.columns[3].name, expect_identifier("type"));
        assert_eq!(apples_def.columns[3].ty, sum_type_ref.into());
        assert_eq!(apples_def.columns[3].default_value, Some(red_delicious));
        assert_eq!(expect_resolve(&def.typespace, &apples_def.columns[3].ty), sum_type);

        assert_eq!(apples_def.primary_key, None);

        assert_eq!(apples_def.constraints.len(), 2);
        let apples_unique_constraint = "Apples_type_key";
        assert_eq!(
            apples_def.constraints[apples_unique_constraint].data,
            ConstraintData::Unique(UniqueConstraintData {
                columns: ColId(3).into()
            })
        );
        assert_eq!(
            &apples_def.constraints[apples_unique_constraint].name[..],
            apples_unique_constraint
        );

        assert_eq!(apples_def.indexes.len(), 3);
        assert_eq!(
            apples_def
                .indexes
                .values()
                .sorted_by_key(|id| &id.name)
                .collect::<Vec<_>>(),
            [
                &IndexDef {
                    name: "Apples_count_idx_direct".into(),
                    accessor_name: Some(expect_identifier("Apples_count_direct")),
                    algorithm: DirectAlgorithm { column: 2.into() }.into(),
                },
                &IndexDef {
                    name: "Apples_name_count_idx_btree".into(),
                    accessor_name: Some(expect_identifier("apples_id")),
                    algorithm: BTreeAlgorithm { columns: [1, 2].into() }.into(),
                },
                &IndexDef {
                    name: "Apples_type_idx_btree".into(),
                    accessor_name: Some(expect_identifier("Apples_type_btree")),
                    algorithm: BTreeAlgorithm { columns: 3.into() }.into(),
                }
            ]
        );

        let bananas_def = &def.tables[&bananas];

        assert_eq!(bananas_def.name, bananas);
        assert_eq!(bananas_def.table_access, TableAccess::Private);
        assert_eq!(bananas_def.table_type, TableType::User);
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
        assert_eq!(bananas_def.indexes.len(), 2);
        assert_eq!(bananas_def.constraints.len(), 1);
        let (bananas_constraint_name, bananas_constraint) = bananas_def.constraints.iter().next().unwrap();
        assert_eq!(bananas_constraint_name, &bananas_constraint.name);
        assert_eq!(
            bananas_constraint.data,
            ConstraintData::Unique(UniqueConstraintData {
                columns: ColId(0).into()
            })
        );

        let delivery_def = &def.tables[&deliveries];
        assert_eq!(delivery_def.name, deliveries);
        assert_eq!(delivery_def.table_access, TableAccess::Public);
        assert_eq!(delivery_def.table_type, TableType::System);
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
        assert_eq!(
            delivery_def.schedule.as_ref().unwrap().function_kind,
            FunctionKind::Reducer
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

        let init_name = expect_identifier("init");
        assert_eq!(def.reducers[&init_name].name, init_name);
        assert_eq!(def.reducers[&init_name].lifecycle, Some(Lifecycle::Init));

        let on_connect_name = expect_identifier("on_connect");
        assert_eq!(def.reducers[&on_connect_name].name, on_connect_name);
        assert_eq!(def.reducers[&on_connect_name].lifecycle, Some(Lifecycle::OnConnect));

        let on_disconnect_name = expect_identifier("on_disconnect");
        assert_eq!(def.reducers[&on_disconnect_name].name, on_disconnect_name);
        assert_eq!(
            def.reducers[&on_disconnect_name].lifecycle,
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
        let mut builder = RawModuleDefV10Builder::new();

        // `build_table` does NOT initialize table.product_type_ref, which should result in an error.
        builder.build_table("Bananas", AlgebraicTypeRef(1337)).finish();

        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::InvalidProductTypeRef { table, ref_ } => {
            &table[..] == "Bananas" && ref_ == &AlgebraicTypeRef(1337)
        });
    }

    #[test]
    fn not_canonically_ordered_columns() {
        let mut builder = RawModuleDefV10Builder::new();
        let product_type = ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]);
        builder
            .build_table_with_new_type("Bananas", product_type.clone(), false)
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::TypeHasIncorrectOrdering { type_name, ref_, bad_type } => {
            type_name == &expect_raw_type_name("Bananas") &&
            ref_ == &AlgebraicTypeRef(0) &&
            bad_type == &product_type.clone().into()
        });
    }

    #[test]
    fn invalid_table_name() {
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type(
                "",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                false,
            )
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::IdentifierError { error } => {
            error == &IdentifierError::Empty {}
        });
    }

    #[test]
    fn invalid_column_name() {
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type(
                "",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                false,
            )
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::IdentifierError { error } => {
            error == &IdentifierError::Empty {}
        });
    }

    #[test]
    fn invalid_index_column_ref() {
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                false,
            )
            .with_index(btree([0, 55]), "bananas_a_b")
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ColumnNotFound { table, def, column } => {
            &table[..] == "Bananas" &&
            &def[..] == "Bananas_b_col_55_idx_btree" &&
            column == &55.into()
        });
    }

    #[test]
    fn invalid_unique_constraint_column_ref() {
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                false,
            )
            .with_unique_constraint(ColId(55))
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ColumnNotFound { table, def, column } => {
            &table[..] == "Bananas" &&
            &def[..] == "Bananas_col_55_key" &&
            column == &55.into()
        });
    }

    #[test]
    fn invalid_sequence_column_ref() {
        // invalid column id
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                false,
            )
            .with_column_sequence(55)
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ColumnNotFound { table, def, column } => {
            &table[..] == "Bananas" &&
            &def[..] == "Bananas_col_55_seq" &&
            column == &55.into()
        });

        // incorrect column type
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::String)]),
                false,
            )
            .with_column_sequence(1)
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::InvalidSequenceColumnType { sequence, column, column_type } => {
            &sequence[..] == "Bananas_a_seq" &&
            column == &RawColumnName::new("Bananas", "a") &&
            column_type.0 == AlgebraicType::String
        });
    }

    #[test]
    fn invalid_index_column_duplicates() {
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                false,
            )
            .with_index(btree([0, 0]), "bananas_b_b")
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateColumns{ def, columns } => {
            &def[..] == "Bananas_b_b_idx_btree" && columns == &ColList::from_iter([0, 0])
        });
    }

    #[test]
    fn invalid_unique_constraint_column_duplicates() {
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                false,
            )
            .with_unique_constraint(ColList::from_iter([1, 1]))
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateColumns{ def, columns } => {
            &def[..] == "Bananas_a_a_key" && columns == &ColList::from_iter([1, 1])
        });
    }

    #[test]
    fn recursive_ref() {
        let recursive_type = AlgebraicType::product([("a", AlgebraicTypeRef(0).into())]);

        let mut builder = RawModuleDefV10Builder::new();
        let ref_ = builder.add_algebraic_type([], "Recursive", recursive_type.clone(), false);
        builder.add_reducer("silly", ProductType::from([("a", ref_.into())]));
        let result: ModuleDef = builder.finish().try_into().unwrap();

        assert!(result.typespace_for_generate[ref_].is_recursive());
    }

    #[test]
    fn out_of_bounds_ref() {
        let invalid_type_1 = AlgebraicType::product([("a", AlgebraicTypeRef(31).into())]);
        let mut builder = RawModuleDefV10Builder::new();
        let ref_ = builder.add_algebraic_type([], "Invalid", invalid_type_1.clone(), false);
        builder.add_reducer("silly", ProductType::from([("a", ref_.into())]));
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ClientCodegenError { location, error: ClientCodegenError::TypeRefError(_)  } => {
            location == &TypeLocation::InTypespace { ref_: AlgebraicTypeRef(0) }
        });
    }

    #[test]
    fn not_valid_for_client_code_generation() {
        let inner_type_invalid_for_use = AlgebraicType::product([("b", AlgebraicType::U32)]);
        let invalid_type = AlgebraicType::product([("a", inner_type_invalid_for_use.clone())]);
        let mut builder = RawModuleDefV10Builder::new();
        let ref_ = builder.add_algebraic_type([], "Invalid", invalid_type.clone(), false);
        builder.add_reducer("silly", ProductType::from([("a", ref_.into())]));
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
    fn hash_index_supported() {
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                true,
            )
            .with_index(hash(0), "bananas_b")
            .finish();
        let def: ModuleDef = builder.finish().try_into().unwrap();
        let indexes = def.indexes().collect::<Vec<_>>();
        assert_eq!(indexes.len(), 1);
        assert_eq!(indexes[0].algorithm, IndexAlgorithm::Hash(0.into()));
    }

    #[test]
    fn unique_constrain_without_index() {
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                false,
            )
            .with_unique_constraint(1)
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(
            result,
            ValidationError::UniqueConstraintWithoutIndex { constraint, columns } => {
                &**constraint == "Bananas_a_key" && *columns == ColSet::from(1)
            }
        );
    }

    #[test]
    fn direct_index_only_u8_to_u64() {
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([("b", AlgebraicType::I32), ("a", AlgebraicType::U64)]),
                false,
            )
            .with_index(direct(0), "bananas_b")
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DirectIndexOnBadType { index, .. } => {
            &index[..] == "Bananas_b_idx_direct"
        });
    }

    #[test]
    fn one_auto_inc() {
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                false,
            )
            .with_column_sequence(1)
            .with_column_sequence(1)
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::OneAutoInc { column } => {
            column == &RawColumnName::new("Bananas", "a")
        });
    }

    #[test]
    fn invalid_primary_key() {
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                false,
            )
            .with_primary_key(44)
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ColumnNotFound { table, def, column } => {
            &table[..] == "Bananas" &&
            &def[..] == "Bananas" &&
            column == &44.into()
        });
    }

    #[test]
    fn missing_primary_key_unique_constraint() {
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                false,
            )
            .with_primary_key(0)
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::MissingPrimaryKeyUniqueConstraint { column } => {
            column == &RawColumnName::new("Bananas", "b")
        });
    }

    #[test]
    fn duplicate_type_name() {
        let mut builder = RawModuleDefV10Builder::new();
        builder.add_algebraic_type(
            ["scope1".into(), "scope2".into()],
            "Duplicate",
            AlgebraicType::U64,
            false,
        );
        builder.add_algebraic_type(
            ["scope1".into(), "scope2".into()],
            "Duplicate",
            AlgebraicType::U32,
            false,
        );
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateTypeName { name } => {
            name == &expect_type_name("scope1::scope2::Duplicate")
        });
    }

    #[test]
    fn duplicate_lifecycle() {
        let mut builder = RawModuleDefV10Builder::new();
        builder.add_lifecycle_reducer(Lifecycle::Init, "init1", ProductType::unit());
        builder.add_lifecycle_reducer(Lifecycle::Init, "init1", ProductType::unit());
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateLifecycle { lifecycle } => {
            lifecycle == &Lifecycle::Init
        });
    }

    #[test]
    fn missing_scheduled_reducer() {
        let mut builder = RawModuleDefV10Builder::new();
        let schedule_at_type = builder.add_type::<ScheduleAt>();
        builder
            .build_table_with_new_type(
                "Deliveries",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("scheduled_at", schedule_at_type.clone()),
                    ("scheduled_id", AlgebraicType::U64),
                ]),
                true,
            )
            .with_auto_inc_primary_key(2)
            .with_index(btree(2), "scheduled_id_index")
            .with_type(TableType::System)
            .finish();

        builder.add_schedule("Deliveries", 1, "check_deliveries");
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::MissingScheduledFunction { schedule, function } => {
            &schedule[..] == "Deliveries_sched" &&
                function == &expect_identifier("check_deliveries")
        });
    }

    #[test]
    fn incorrect_scheduled_reducer_args() {
        let mut builder = RawModuleDefV10Builder::new();
        let schedule_at_type = builder.add_type::<ScheduleAt>();
        let deliveries_product_type = builder
            .build_table_with_new_type(
                "Deliveries",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("scheduled_at", schedule_at_type.clone()),
                    ("scheduled_id", AlgebraicType::U64),
                ]),
                true,
            )
            .with_auto_inc_primary_key(2)
            .with_index(direct(2), "scheduled_id_idx")
            .with_type(TableType::System)
            .finish();

        builder.add_schedule("Deliveries", 1, "check_deliveries");
        builder.add_reducer("check_deliveries", ProductType::from([("a", AlgebraicType::U64)]));
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::IncorrectScheduledFunctionParams { function_name, function_kind, expected, actual } => {
            &function_name[..] == "check_deliveries" &&
                *function_kind == FunctionKind::Reducer &&
                expected.0 == AlgebraicType::product([AlgebraicType::Ref(deliveries_product_type)]) &&
                actual.0 == ProductType::from([("a", AlgebraicType::U64)]).into()
        });
    }

    #[test]
    fn wacky_names() {
        let mut builder = RawModuleDefV10Builder::new();

        let schedule_at_type = builder.add_type::<ScheduleAt>();

        let deliveries_product_type = builder
            .build_table_with_new_type(
                "Deliveries",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("scheduled_at", schedule_at_type.clone()),
                    ("scheduled_id", AlgebraicType::U64),
                ]),
                true,
            )
            .with_auto_inc_primary_key(2)
            .with_index(direct(2), "scheduled_id_index")
            .with_index(btree([0, 2]), "nice_index_name")
            .with_type(TableType::System)
            .finish();

        builder.add_schedule("Deliveries", 1, "check_deliveries");
        builder.add_reducer(
            "check_deliveries",
            ProductType::from([("a", deliveries_product_type.into())]),
        );

        // Our builder methods ignore the possibility of setting names at the moment.
        // But, it could be done in the future for some reason.
        // Check if it works.
        let mut raw_def = builder.finish();
        let tables = raw_def.tables_mut_for_tests();
        tables[0].constraints[0].name = Some("wacky.constraint()".into());
        tables[0].indexes[0].name = Some("wacky.index()".into());
        tables[0].sequences[0].name = Some("wacky.sequence()".into());

        let def: ModuleDef = raw_def.try_into().unwrap();
        assert!(def.lookup::<ConstraintDef>("wacky.constraint()").is_some());
        assert!(def.lookup::<IndexDef>("wacky.index()").is_some());
        assert!(def.lookup::<SequenceDef>("wacky.sequence()").is_some());
    }

    #[test]
    fn duplicate_reducer_names() {
        let mut builder = RawModuleDefV10Builder::new();

        builder.add_reducer("foo", [("i", AlgebraicType::I32)].into());
        builder.add_reducer("foo", [("name", AlgebraicType::String)].into());

        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateFunctionName { name } => {
            &name[..] == "foo"
        });
    }

    #[test]
    fn duplicate_procedure_names() {
        let mut builder = RawModuleDefV10Builder::new();

        builder.add_procedure("foo", [("i", AlgebraicType::I32)].into(), AlgebraicType::unit());
        builder.add_procedure("foo", [("name", AlgebraicType::String)].into(), AlgebraicType::unit());

        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateFunctionName { name } => {
            &name[..] == "foo"
        });
    }

    #[test]
    fn duplicate_procedure_and_reducer_name() {
        let mut builder = RawModuleDefV10Builder::new();

        builder.add_reducer("foo", [("i", AlgebraicType::I32)].into());
        builder.add_procedure("foo", [("i", AlgebraicType::I32)].into(), AlgebraicType::unit());

        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateFunctionName { name } => {
            &name[..] == "foo"
        });
    }
}
