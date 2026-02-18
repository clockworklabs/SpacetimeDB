use spacetimedb_data_structures::map::HashMap;
use spacetimedb_lib::bsatn::Deserializer;
use spacetimedb_lib::db::raw_def::v10::*;
use spacetimedb_lib::de::DeserializeSeed as _;
use spacetimedb_sats::{Typespace, WithTypespace};

use crate::def::validate::v9::{
    check_function_names_are_unique, check_scheduled_functions_exist, generate_schedule_name,
    generate_unique_constraint_name, identifier, CoreValidator, TableValidator, ViewValidator,
};
use crate::def::*;
use crate::error::ValidationError;
use crate::type_for_generate::ProductTypeDef;
use crate::{def::validate::Result, error::TypeLocation};

// Utitility struct to look up canonical names for tables, functions, and indexes based on the
// explicit names provided in the `RawModuleDefV10`.
#[derive(Default)]
pub struct ExplicitNamesLookup {
    pub tables: HashMap<RawIdentifier, RawIdentifier>,
    pub functions: HashMap<RawIdentifier, RawIdentifier>,
    pub indexes: HashMap<RawIdentifier, RawIdentifier>,
}

impl ExplicitNamesLookup {
    fn new(ex: ExplicitNames) -> Self {
        let mut tables = HashMap::default();
        let mut functions = HashMap::default();
        let mut indexes = HashMap::default();

        for entry in ex.into_entries() {
            match entry {
                ExplicitNameEntry::Table(m) => {
                    tables.insert(m.source_name, m.canonical_name);
                }
                ExplicitNameEntry::Function(m) => {
                    functions.insert(m.source_name, m.canonical_name);
                }
                ExplicitNameEntry::Index(m) => {
                    indexes.insert(m.source_name, m.canonical_name);
                }
                _ => {}
            }
        }

        ExplicitNamesLookup {
            tables,
            functions,
            indexes,
        }
    }
}

/// Validate a `RawModuleDefV9` and convert it into a `ModuleDef`,
/// or return a stream of errors if the definition is invalid.
pub fn validate(def: RawModuleDefV10) -> Result<ModuleDef> {
    let mut typespace = def.typespace().cloned().unwrap_or_else(|| Typespace::EMPTY.clone());
    let known_type_definitions = def.types().into_iter().flatten().map(|def| def.ty);
    let case_policy = def.case_conversion_policy();
    let explicit_names = def
        .explicit_names()
        .cloned()
        .map(ExplicitNamesLookup::new)
        .unwrap_or_default();

    // Original `typespace` needs to be preserved to be assign `accesor_name`s to columns.
    let typespace_with_accessor_names = typespace.clone();
    // Apply case conversion to `typespace`.
    CoreValidator::typespace_case_conversion(case_policy, &mut typespace);

    let mut validator = ModuleValidatorV10 {
        core: CoreValidator {
            typespace: &typespace,
            stored_in_table_def: Default::default(),
            type_namespace: Default::default(),
            lifecycle_reducers: Default::default(),
            typespace_for_generate: TypespaceForGenerate::builder(&typespace, known_type_definitions),
            case_policy,
            explicit_names,
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
        .map(|reducer| validator.validate_reducer_def(reducer))
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
                .validate_view_def(view, &typespace_with_accessor_names)
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
                .validate_table_def(table, &typespace_with_accessor_names)
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
            validator.core.validate_type_def(ty.into()).map(|type_def| {
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
                    let function_name = ReducerName::new(
                        validator
                            .core
                            .resolve_function_ident(lifecycle_def.function_name.clone())?,
                    );

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
                let (mut reducers, mut procedures, views) =
                    check_function_names_are_unique(reducers, procedures, views)?;
                // Attach lifecycles to their respective reducers
                attach_lifecycles_to_reducers(&mut reducers, lifecycles)?;

                // Attach schedules to their respective tables
                attach_schedules_to_tables(&mut tables, schedules)?;

                check_scheduled_functions_exist(&mut tables, &reducers, &procedures)?;
                change_scheduled_functions_and_lifetimes_visibility(&tables, &mut reducers, &mut procedures)?;

                Ok((tables, types, reducers, procedures, views))
            },
        );
    let CoreValidator {
        stored_in_table_def,
        typespace_for_generate,
        lifecycle_reducers,
        ..
    } = validator.core;

    let row_level_security_raw = def
        .row_level_security()
        .into_iter()
        .flatten()
        .map(|rls| (rls.sql.clone(), rls.to_owned()))
        .collect();

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
        row_level_security_raw,
        lifecycle_reducers,
        procedures,
        raw_module_def_version: RawModuleDefVersion::V10,
    })
}

/// Change the visibility of scheduled functions and lifecycle reducers to Internal.
///
fn change_scheduled_functions_and_lifetimes_visibility(
    tables: &HashMap<Identifier, TableDef>,
    reducers: &mut IndexMap<Identifier, ReducerDef>,
    procedures: &mut IndexMap<Identifier, ProcedureDef>,
) -> Result<()> {
    for sched_def in tables.iter().filter_map(|(_, t)| t.schedule.as_ref()) {
        match sched_def.function_kind {
            FunctionKind::Reducer => {
                let def = reducers.get_mut(&sched_def.function_name).ok_or_else(|| {
                    ValidationError::MissingScheduledFunction {
                        schedule: sched_def.name.clone(),
                        function: sched_def.function_name.clone(),
                    }
                })?;

                def.visibility = crate::def::FunctionVisibility::Private;
            }

            FunctionKind::Procedure => {
                let def = procedures.get_mut(&sched_def.function_name).ok_or_else(|| {
                    ValidationError::MissingScheduledFunction {
                        schedule: sched_def.name.clone(),
                        function: sched_def.function_name.clone(),
                    }
                })?;

                def.visibility = crate::def::FunctionVisibility::Private;
            }

            FunctionKind::Unknown => {}
        }
    }

    for red_def in reducers.iter_mut().map(|(_, r)| r) {
        if red_def.lifecycle.is_some() {
            red_def.visibility = crate::def::FunctionVisibility::Private;
        }
    }

    Ok(())
}

struct ModuleValidatorV10<'a> {
    core: CoreValidator<'a>,
}

impl<'a> ModuleValidatorV10<'a> {
    fn validate_table_def(&mut self, table: RawTableDefV10, typespace_with_accessor: &Typespace) -> Result<TableDef> {
        let RawTableDefV10 {
            source_name: raw_table_name,
            product_type_ref,
            primary_key,
            indexes,
            constraints,
            sequences,
            table_type,
            table_access,
            default_values,
            is_event,
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
            TableValidator::new(raw_table_name.clone(), product_type_ref, product_type, &mut self.core)?;

        let table_ident = table_validator.table_ident.clone();

        // Validate columns first
        let mut columns: Vec<ColumnDef> = (0..product_type.elements.len())
            .map(|id| {
                let product_type_for_column: &ProductType = typespace_with_accessor
                    .get(product_type_ref)
                    .and_then(AlgebraicType::as_product)
                    .ok_or_else(|| {
                        ValidationErrors::from(ValidationError::InvalidProductTypeRef {
                            table: raw_table_name.clone(),
                            ref_: product_type_ref,
                        })
                    })?;

                table_validator.validate_column_def(id.into(), product_type_for_column)
            })
            .collect_all_errors()?;

        let indexes = indexes
            .into_iter()
            .map(|index| {
                table_validator
                    .validate_index_def_v10(index)
                    .map(|index| (index.name.clone(), index))
            })
            .collect_all_errors::<StrMap<_>>();

        let constraints_primary_key = constraints
            .into_iter()
            .map(|constraint| {
                table_validator
                    .validate_constraint_def(constraint.into(), |_source_name, cols| {
                        generate_unique_constraint_name(&table_ident, product_type, cols)
                    })
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
                    .validate_sequence_def(sequence.into())
                    .map(|sequence| (sequence.name.clone(), sequence))
            })
            .collect_all_errors();

        // `raw_table_name` should also go in global namespace as it will be used as alias
        let raw_table_name = table_validator.add_to_global_namespace(raw_table_name.clone())?;

        let name = {
            let name = table_validator
                .module_validator
                .resolve_table_ident(raw_table_name.clone())?;
            if table_type != TableType::System && name.starts_with("st_") {
                Err(ValidationError::TableNameReserved { table: name }.into())
            } else {
                let mut name = name.as_raw().clone();
                if name != raw_table_name {
                    name = table_validator.add_to_global_namespace(name)?;
                }

                Ok(name)
            }
        };

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
            name: identifier(name)?,
            product_type_ref,
            primary_key,
            columns,
            indexes,
            constraints,
            sequences,
            schedule: None, // V10 handles schedules separately
            table_type,
            table_access,
            is_event,
            accessor_name: identifier(raw_table_name)?,
        })
    }

    fn validate_reducer_def(&mut self, reducer_def: RawReducerDefV10) -> Result<(Identifier, ReducerDef)> {
        let RawReducerDefV10 {
            source_name,
            params,
            visibility,
            ok_return_type,
            err_return_type,
        } = reducer_def;

        let params_for_generate =
            self.core
                .params_for_generate(&params, |position, arg_name| TypeLocation::ReducerArg {
                    reducer_name: source_name.clone(),
                    position,
                    arg_name,
                });

        let name_result = self.core.resolve_function_ident(source_name.clone());

        let return_res: Result<_> = (ok_return_type.is_unit() && err_return_type.is_string())
            .then_some((ok_return_type.clone(), err_return_type.clone()))
            .ok_or_else(move || {
                ValidationError::InvalidReducerReturnType {
                    reducer_name: source_name.clone(),
                    ok_type: ok_return_type.into(),
                    err_type: err_return_type.into(),
                }
                .into()
            });

        let (name_result, params_for_generate, return_res) =
            (name_result, params_for_generate, return_res).combine_errors()?;
        let (ok_return_type, err_return_type) = return_res;

        Ok(ReducerDef {
            name: ReducerName::new(name_result.clone()),
            params: params.clone(),
            params_for_generate: ProductTypeDef {
                elements: params_for_generate,
                recursive: false, // A ProductTypeDef not stored in a Typespace cannot be recursive.
            },
            lifecycle: None, // V10 handles lifecycle separately
            visibility: visibility.into(),
            ok_return_type,
            err_return_type,
        })
        .map(|reducer_def| (name_result, reducer_def))
    }

    fn validate_schedule_def(
        &mut self,
        schedule: RawScheduleDefV10,
        tables: &HashMap<Identifier, TableDef>,
    ) -> Result<(ScheduleDef, Identifier)> {
        let RawScheduleDefV10 {
            source_name: _,
            table_name,
            schedule_at_col,
            function_name,
        } = schedule;

        let table_ident = self.core.resolve_table_ident(table_name.clone())?;

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

        let source_name = generate_schedule_name(&table_ident);
        self.core
            .validate_schedule_def(
                table_name.clone(),
                source_name,
                function_name,
                product_type,
                schedule_at_col,
                table.primary_key,
            )
            .map(|schedule_def| (schedule_def, table_ident))
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
            source_name,
            params,
            return_type,
            visibility,
        } = procedure_def;

        let params_for_generate =
            self.core
                .params_for_generate(&params, |position, arg_name| TypeLocation::ProcedureArg {
                    procedure_name: source_name.clone(),
                    position,
                    arg_name,
                });

        let return_type_for_generate = self.core.validate_for_type_use(
            || TypeLocation::ProcedureReturn {
                procedure_name: source_name.clone(),
            },
            &return_type,
        );

        let name_result = self.core.resolve_function_ident(source_name);

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
            visibility: visibility.into(),
        })
    }

    fn validate_view_def(&mut self, view_def: RawViewDefV10, typespace_with_accessor: &Typespace) -> Result<ViewDef> {
        let RawViewDefV10 {
            source_name: accessor_name,
            is_public,
            is_anonymous,
            params,
            return_type,
            index,
        } = view_def;

        let invalid_return_type = || {
            ValidationErrors::from(ValidationError::InvalidViewReturnType {
                view: accessor_name.clone(),
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
                    table: accessor_name.clone(),
                    ref_: product_type_ref,
                })
            })?;

        let params_for_generate =
            self.core
                .params_for_generate(&params, |position, arg_name| TypeLocation::ViewArg {
                    view_name: accessor_name.clone(),
                    position,
                    arg_name,
                })?;

        let return_type_for_generate = self.core.validate_for_type_use(
            || TypeLocation::ViewReturn {
                view_name: accessor_name.clone(),
            },
            &return_type,
        );

        let name = self.core.resolve_function_ident(accessor_name.clone())?;

        let mut view_validator = ViewValidator::new(
            accessor_name.clone(),
            product_type_ref,
            product_type,
            &params,
            &params_for_generate,
            &mut self.core,
        )?;

        let _ = view_validator.add_to_global_namespace(name.as_raw().clone())?;

        let n = product_type.elements.len();
        let return_columns = (0..n)
            .map(|id| {
                let product_type = typespace_with_accessor
                    .get(product_type_ref)
                    .and_then(AlgebraicType::as_product)
                    .ok_or_else(|| {
                        ValidationErrors::from(ValidationError::InvalidProductTypeRef {
                            table: accessor_name.clone(),
                            ref_: product_type_ref,
                        })
                    })?;
                view_validator.validate_view_column_def(id.into(), product_type)
            })
            .collect_all_errors();

        let n = params.elements.len();
        let param_columns = (0..n)
            .map(|id| view_validator.validate_param_column_def(id.into()))
            .collect_all_errors();

        let (return_type_for_generate, return_columns, param_columns) =
            (return_type_for_generate, return_columns, param_columns).combine_errors()?;

        Ok(ViewDef {
            name,
            accessor_name: identifier(accessor_name)?,
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
    schedules: Vec<(ScheduleDef, Identifier)>,
) -> Result<()> {
    for schedule in schedules {
        let (schedule, table_name) = schedule;
        let table = tables.values_mut().find(|t| *t.name == *table_name).ok_or_else(|| {
            ValidationError::MissingScheduleTable {
                table_name: table_name.as_raw().clone(),
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
        BTreeAlgorithm, ConstraintData, DirectAlgorithm, FunctionKind, FunctionVisibility, IndexAlgorithm, IndexDef,
        UniqueConstraintData,
    };
    use crate::error::*;
    use crate::identifier::Identifier;
    use crate::type_for_generate::ClientCodegenError;

    use itertools::Itertools;
    use spacetimedb_data_structures::expect_error_matching;
    use spacetimedb_lib::db::raw_def::v10::{CaseConversionPolicy, RawModuleDefV10Builder};
    use spacetimedb_lib::db::raw_def::v9::{btree, direct, hash};
    use spacetimedb_lib::db::raw_def::*;
    use spacetimedb_lib::ScheduleAt;
    use spacetimedb_primitives::{ColId, ColList, ColSet};
    use spacetimedb_sats::{AlgebraicType, AlgebraicTypeRef, AlgebraicValue, ProductType, SumValue};
    use v9::{Lifecycle, TableAccess, TableType};

    /// This test attempts to exercise every successful path in the validation code.
    #[test]
    fn test_valid_definition_with_default_policy() {
        let mut builder = RawModuleDefV10Builder::new();

        let product_type = AlgebraicType::product([("a", AlgebraicType::U64), ("b", AlgebraicType::String)]);
        let product_type_ref = builder.add_algebraic_type(
            ["Scope1".into(), "Scope2".into()],
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
                    ("Apple_name", AlgebraicType::String),
                    ("countFresh", AlgebraicType::U16),
                    ("type", sum_type_ref.into()),
                ]),
                true,
            )
            .with_index_no_accessor_name(btree([1, 2]), "apples_id")
            .with_index_no_accessor_name(direct(2), "Apples_count_direct")
            .with_unique_constraint(2)
            .with_index_no_accessor_name(btree(3), "Apples_type_btree")
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
            .with_index_no_accessor_name(btree(0), "bananas_count")
            .with_index_no_accessor_name(btree([0, 1, 2]), "bananas_count_id_name")
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
            .with_index_no_accessor_name(btree(2), "scheduled_id_index")
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

        let casing_policy = CaseConversionPolicy::default();
        assert_eq!(casing_policy, CaseConversionPolicy::SnakeCase);
        let apples = Identifier::for_test("apples");
        let bananas = Identifier::for_test("bananas");
        let deliveries = Identifier::for_test("deliveries");

        assert_eq!(def.tables.len(), 3);

        let apples_def = &def.tables[&apples];

        assert_eq!(apples_def.name, apples);
        assert_eq!(apples_def.table_type, TableType::User);
        assert_eq!(apples_def.table_access, TableAccess::Public);

        assert_eq!(apples_def.columns.len(), 4);
        assert_eq!(apples_def.columns[0].name, expect_identifier("id"));
        assert_eq!(apples_def.columns[0].ty, AlgebraicType::U64);
        assert_eq!(apples_def.columns[0].default_value, None);
        assert_eq!(apples_def.columns[1].name, expect_identifier("apple_name"));
        assert_eq!(apples_def.columns[1].ty, AlgebraicType::String);
        assert_eq!(apples_def.columns[1].default_value, None);
        assert_eq!(apples_def.columns[2].name, expect_identifier("count_fresh"));
        assert_eq!(apples_def.columns[2].ty, AlgebraicType::U16);
        assert_eq!(apples_def.columns[2].default_value, Some(AlgebraicValue::U16(37)));
        assert_eq!(apples_def.columns[3].name, expect_identifier("type"));
        assert_eq!(apples_def.columns[3].ty, sum_type_ref.into());
        assert_eq!(apples_def.columns[3].default_value, Some(red_delicious));
        let expected_sum_type = AlgebraicType::simple_enum(["gala", "grannySmith", "redDelicious"].into_iter());
        assert_eq!(
            expect_resolve(&def.typespace, &apples_def.columns[3].ty),
            expected_sum_type
        );

        assert_eq!(apples_def.primary_key, None);

        assert_eq!(apples_def.constraints.len(), 2);
        let apples_unique_constraint = "apples_type_key";
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
                    name: "apples_apple_name_count_fresh_idx_btree".into(),
                    accessor_name: "apples_id".into(),
                    codegen_name: Some(expect_identifier("apples_apple_name_count_fresh_idx_btree")),
                    algorithm: BTreeAlgorithm {
                        columns: [ColId(1), ColId(2)].into(),
                    }
                    .into(),
                },
                &IndexDef {
                    name: "apples_count_fresh_idx_direct".into(),
                    accessor_name: "Apples_count_direct".into(),
                    codegen_name: Some(expect_identifier("apples_count_fresh_idx_direct")),
                    algorithm: DirectAlgorithm { column: ColId(2) }.into()
                },
                &IndexDef {
                    name: "apples_type_idx_btree".into(),
                    accessor_name: "Apples_type_btree".into(),
                    codegen_name: Some(expect_identifier("apples_type_idx_btree")),
                    algorithm: BTreeAlgorithm {
                        columns: [ColId(3)].into()
                    }
                    .into()
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
        assert_eq!(def.typespace.get(sum_type_ref), Some(&expected_sum_type));

        check_product_type(&def, apples_def);
        check_product_type(&def, bananas_def);
        check_product_type(&def, delivery_def);

        let product_type_name = expect_type_name("Scope1::Scope2::ReferencedProduct");
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
        assert_eq!(&*def.reducers[&init_name].name, &*init_name);
        assert_eq!(def.reducers[&init_name].lifecycle, Some(Lifecycle::Init));

        let on_connect_name = expect_identifier("on_connect");
        assert_eq!(&*def.reducers[&on_connect_name].name, &*on_connect_name);
        assert_eq!(def.reducers[&on_connect_name].lifecycle, Some(Lifecycle::OnConnect));

        let on_disconnect_name = expect_identifier("on_disconnect");
        assert_eq!(&*def.reducers[&on_disconnect_name].name, &*on_disconnect_name);
        assert_eq!(
            def.reducers[&on_disconnect_name].lifecycle,
            Some(Lifecycle::OnDisconnect)
        );

        let extra_reducer_name = expect_identifier("extra_reducer");
        assert_eq!(&*def.reducers[&extra_reducer_name].name, &*extra_reducer_name);
        assert_eq!(def.reducers[&extra_reducer_name].lifecycle, None);
        assert_eq!(
            def.reducers[&extra_reducer_name].params,
            ProductType::from([("a", AlgebraicType::U64)])
        );

        let check_deliveries_name = expect_identifier("check_deliveries");
        assert_eq!(&*def.reducers[&check_deliveries_name].name, &*check_deliveries_name);
        assert_eq!(def.reducers[&check_deliveries_name].lifecycle, None);
        assert_eq!(
            def.reducers[&check_deliveries_name].params,
            ProductType::from([("a", deliveries_product_type.into())])
        );

        assert_eq!(
            def.reducers[&check_deliveries_name].visibility,
            FunctionVisibility::Private,
        );
        assert_eq!(def.reducers[&init_name].visibility, FunctionVisibility::Private);
        assert_eq!(
            def.reducers[&extra_reducer_name].visibility,
            FunctionVisibility::ClientCallable
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
                true,
            )
            .with_index_no_accessor_name(btree([0, 55]), "Bananas_a_b")
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ColumnNotFound { table, def, column } => {
            &table[..] == "Bananas" &&
            &def[..] == "bananas_b_col_55_idx_btree" &&
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
                true,
            )
            .with_unique_constraint(ColId(55))
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ColumnNotFound { table, def, column } => {
            &table[..] == "Bananas" &&
            &def[..] == "bananas_col_55_key" &&
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
                true,
            )
            .with_column_sequence(55)
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ColumnNotFound { table, def, column } => {
            &table[..] == "Bananas" &&
            &def[..] == "bananas_col_55_seq" &&
            column == &55.into()
        });

        // incorrect column type
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::String)]),
                true,
            )
            .with_column_sequence(1)
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::InvalidSequenceColumnType { sequence, column, column_type } => {
            &sequence[..] == "bananas_a_seq" &&
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
                true,
            )
            .with_index_no_accessor_name(btree([0, 0]), "bananas_b_b")
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateColumns{ def, columns } => {
            &def[..] == "bananas_b_b_idx_btree" && columns == &ColList::from_iter([0, 0])
        });
    }

    #[test]
    fn invalid_unique_constraint_column_duplicates() {
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([("a", AlgebraicType::U16), ("b", AlgebraicType::U64)]),
                true,
            )
            .with_unique_constraint(ColList::from_iter([1, 1]))
            .with_unique_constraint(ColList::from_iter([1, 1]))
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateColumns{ def, columns } => {
            &def[..] == "bananas_b_b_key" && columns == &ColList::from_iter([1, 1])
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
            .with_index_no_accessor_name(hash(0), "bananas_b")
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
                ProductType::from([("a", AlgebraicType::U16), ("b", AlgebraicType::U64)]),
                true,
            )
            .with_unique_constraint(1)
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(
            result,
            ValidationError::UniqueConstraintWithoutIndex { constraint, columns } => {
                &**constraint == "bananas_b_key" && *columns == ColSet::from(1)
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
            .with_index_no_accessor_name(direct(0), "bananas_b")
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DirectIndexOnBadType { index, .. } => {
            &index[..] == "bananas_b_idx_direct"
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
            name == &expect_type_name("Scope1::Scope2::Duplicate")
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
            .with_index_no_accessor_name(btree(2), "scheduled_id_index")
            .with_type(TableType::System)
            .finish();

        builder.add_schedule("Deliveries", 1, "check_deliveries");
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::MissingScheduledFunction { schedule, function } => {
            &schedule[..] == "deliveries_sched" &&
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
            .with_index_no_accessor_name(direct(2), "scheduled_id_idx")
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

    fn make_case_conversion_builder() -> (RawModuleDefV10Builder, AlgebraicTypeRef) {
        let mut builder = RawModuleDefV10Builder::new();

        // Sum type: PascalCase variants → camelCase after conversion.
        let color_sum = AlgebraicType::simple_enum(["RedApple", "GreenApple", "YellowApple"].into_iter());
        let color_ref = builder.add_algebraic_type([], "FruitColor", color_sum, true);

        // Product type with scope: scope segments stay unchanged, unscoped name → PascalCase.
        builder.add_algebraic_type(
            ["myLib".into(), "utils".into()],
            "metaInfo",
            AlgebraicType::product([("kind", AlgebraicType::U8)]),
            false,
        );

        // Table 1: "FruitBasket"
        //   [0] BasketId     PascalCase → "basket_id"
        //   [1] fruitName    camelCase  → "fruit_name"
        //   [2] ItemCount    PascalCase → "item_count"
        //   [3] color_label  snake_case → "color_label"
        builder
            .build_table_with_new_type(
                "FruitBasket",
                ProductType::from([
                    ("BasketId", AlgebraicType::U64),
                    ("fruitName", AlgebraicType::String),
                    ("ItemCount", AlgebraicType::U32),
                    ("color_label", color_ref.into()),
                ]),
                true,
            )
            .with_index_no_accessor_name(btree([0, 1]), "RawBasketLookup")
            .with_index_no_accessor_name(direct(2), "RawCountDirect")
            .with_unique_constraint(ColId(2))
            .with_column_sequence(0)
            .finish();

        // Table 2: "deliveryRecord"
        //   [0] recordId    camelCase  → "record_id"
        //   [1] ScheduledAt PascalCase → "scheduled_at"
        //   [2] SeqId       PascalCase → "seq_id"
        let schedule_at_type = builder.add_type::<spacetimedb_lib::ScheduleAt>();

        let builder_type_ref = builder
            .build_table_with_new_type(
                "deliveryRecord",
                ProductType::from([
                    ("recordId", AlgebraicType::U64),
                    ("ScheduledAt", schedule_at_type),
                    ("SeqId", AlgebraicType::U64),
                ]),
                true,
            )
            .with_auto_inc_primary_key(2)
            .with_index_no_accessor_name(btree(2), "SeqIdIndex")
            .with_type(TableType::System)
            .finish();

        builder.add_reducer("doDelivery", ProductType::from([("a", builder_type_ref.into())]));
        builder.add_reducer("ProcessItem", ProductType::from([("b", AlgebraicType::U32)]));
        builder.add_schedule("deliveryRecord", 1, "doDelivery");

        (builder, color_ref)
    }

    /// Exhaustive test for case-conversion under the default [`CaseConversionPolicy::SnakeCase`].
    ///
    /// Rules under verification:
    ///
    /// | Entity          | Source style     | Canonical style           | Notes                          |
    /// |-----------------|------------------|---------------------------|--------------------------------|
    /// | Table name      | any              | snake_case                | raw name preserved as accessor |
    /// | Column name     | any              | snake_case                | raw name preserved as accessor |
    /// | Reducer name    | any              | snake_case                | —                              |
    /// | Type name       | any (unscoped)   | PascalCase                | scope segments unchanged       |
    /// | Enum variant    | any              | camelCase                 | —                              |
    /// | Index name      | autogenerated    | `{tbl}_{cols}_idx_{algo}` | uses canonical table+col names |
    /// | Index accessor  | raw source_name  | **unchanged**             | no conversion applied          |
    /// | Constraint name | autogenerated    | `{tbl}_{cols}_key`        | uses canonical table+col names |
    /// | Sequence name   | autogenerated    | `{tbl}_{col}_seq`         | uses canonical table+col names |
    /// | Schedule name   | autogenerated    | `{tbl}_sched`             | uses canonical table name      |
    #[test]
    fn test_case_conversion_snake_case_policy() {
        use crate::def::*;
        use crate::identifier::Identifier;
        use itertools::Itertools;
        use spacetimedb_lib::db::raw_def::v10::CaseConversionPolicy;
        use spacetimedb_sats::AlgebraicType;

        let id = |s: &str| Identifier::for_test(s);

        let (builder, color_ref) = make_case_conversion_builder();
        let def: ModuleDef = builder.finish().try_into().unwrap();

        // Sanity: policy is SnakeCase by default.
        assert_eq!(CaseConversionPolicy::default(), CaseConversionPolicy::SnakeCase);

        // ═══════════════════════════════════════════════════════════════════════════
        // TABLE NAMES
        // ═══════════════════════════════════════════════════════════════════════════

        assert_eq!(def.tables.len(), 2);

        // "FruitBasket" → canonical "fruit_basket"
        let fruit_basket = id("fruit_basket");
        assert!(def.tables.contains_key(&fruit_basket), "table 'fruit_basket' not found");
        let fb = &def.tables[&fruit_basket];
        assert_eq!(fb.name, fruit_basket, "table canonical name");
        assert_eq!(
            &*fb.accessor_name, "FruitBasket",
            "table accessor_name must preserve raw source"
        );

        // "deliveryRecord" → canonical "delivery_record"
        let delivery_record = id("delivery_record");
        assert!(
            def.tables.contains_key(&delivery_record),
            "table 'delivery_record' not found"
        );
        let dr = &def.tables[&delivery_record];
        assert_eq!(dr.name, delivery_record, "table canonical name");
        assert_eq!(
            &*dr.accessor_name, "deliveryRecord",
            "table accessor_name must preserve raw source"
        );

        // ═══════════════════════════════════════════════════════════════════════════
        // COLUMN NAMES — FruitBasket
        // ═══════════════════════════════════════════════════════════════════════════

        assert_eq!(fb.columns.len(), 4);

        // [0] "BasketId" (PascalCase) → "basket_id"
        assert_eq!(fb.columns[0].name, id("basket_id"), "col 0 canonical");
        assert_eq!(&*fb.columns[0].accessor_name, "BasketId", "col 0 accessor");
        assert_eq!(fb.columns[0].ty, AlgebraicType::U64);

        // [1] "fruitName" (camelCase) → "fruit_name"
        assert_eq!(fb.columns[1].name, id("fruit_name"), "col 1 canonical");
        assert_eq!(&*fb.columns[1].accessor_name, "fruitName", "col 1 accessor");
        assert_eq!(fb.columns[1].ty, AlgebraicType::String);

        // [2] "ItemCount" (PascalCase) → "item_count"
        assert_eq!(fb.columns[2].name, id("item_count"), "col 2 canonical");
        assert_eq!(&*fb.columns[2].accessor_name, "ItemCount", "col 2 accessor");
        assert_eq!(fb.columns[2].ty, AlgebraicType::U32);

        // [3] "color_label" (already snake) → "color_label"
        assert_eq!(fb.columns[3].name, id("color_label"), "col 3 canonical");
        assert_eq!(&*fb.columns[3].accessor_name, "color_label", "col 3 accessor");

        // ═══════════════════════════════════════════════════════════════════════════
        // COLUMN NAMES — deliveryRecord
        // ═══════════════════════════════════════════════════════════════════════════

        assert_eq!(dr.columns.len(), 3);

        // [0] "recordId" (camelCase) → "record_id"
        assert_eq!(dr.columns[0].name, id("record_id"), "dr col 0 canonical");
        assert_eq!(&*dr.columns[0].accessor_name, "recordId", "dr col 0 accessor");

        // [1] "ScheduledAt" (PascalCase) → "scheduled_at"
        assert_eq!(dr.columns[1].name, id("scheduled_at"), "dr col 1 canonical");
        assert_eq!(&*dr.columns[1].accessor_name, "ScheduledAt", "dr col 1 accessor");

        // [2] "SeqId" (PascalCase) → "seq_id"
        assert_eq!(dr.columns[2].name, id("seq_id"), "dr col 2 canonical");
        assert_eq!(&*dr.columns[2].accessor_name, "SeqId", "dr col 2 accessor");

        // ═══════════════════════════════════════════════════════════════════════════
        // REDUCER NAMES
        // ═══════════════════════════════════════════════════════════════════════════

        // "doDelivery" (camelCase) → "do_delivery"
        let do_delivery = id("do_delivery");
        assert!(
            def.reducers.contains_key(&do_delivery),
            "reducer 'do_delivery' not found"
        );
        assert_eq!(def.reducers[&do_delivery].name.as_identifier(), &do_delivery);

        // "ProcessItem" (PascalCase) → "process_item"
        let process_item = id("process_item");
        assert!(
            def.reducers.contains_key(&process_item),
            "reducer 'process_item' not found"
        );
        assert_eq!(def.reducers[&process_item].name.as_identifier(), &process_item);

        // ═══════════════════════════════════════════════════════════════════════════
        // TYPE NAMES — PascalCase; scoped names keep their scope segments unchanged
        // ═══════════════════════════════════════════════════════════════════════════

        // "FruitColor" (already Pascal) → "FruitColor"
        assert!(
            def.types.contains_key(&expect_type_name("FruitColor")),
            "type 'FruitColor' not found"
        );

        // "metaInfo" (lower-camel unscoped) → "MetaInfo"; scope "myLib","utils" → unchanged
        assert!(
            def.types.contains_key(&expect_type_name("MyLib::Utils::MetaInfo")),
            "type 'myLib::utils::MetaInfo' not found"
        );

        // Anonymous table types keep the raw source name as-is.
        assert!(def.types.contains_key(&expect_type_name("FruitBasket")));
        assert!(
            def.types.contains_key(&expect_type_name("deliveryRecord"))
                || def.types.contains_key(&expect_type_name("DeliveryRecord")),
            "anonymous type for deliveryRecord not found"
        );

        // ═══════════════════════════════════════════════════════════════════════════
        // ENUM VARIANT NAMES — camelCase
        // ═══════════════════════════════════════════════════════════════════════════

        // "RedApple" → "redApple", "GreenApple" → "greenApple", "YellowApple" → "yellowApple"
        let expected_color_sum = AlgebraicType::simple_enum(["redApple", "greenApple", "yellowApple"].into_iter());
        assert_eq!(
            def.typespace.get(color_ref),
            Some(&expected_color_sum),
            "enum variants should be camelCase"
        );

        // ═══════════════════════════════════════════════════════════════════════════
        // INDEX NAMES — autogenerated from canonical table + canonical column names
        // ═══════════════════════════════════════════════════════════════════════════
        //
        // "FruitBasket" → "fruit_basket"; cols [0]="basket_id" [1]="fruit_name" [2]="item_count"
        //   btree([0,1]) → "fruit_basket_basket_id_fruit_name_idx_btree"
        //   direct(2)    → "fruit_basket_item_count_idx_direct"
        //
        // accessor_name = raw source_name passed to with_index(), never converted.

        assert_eq!(fb.indexes.len(), 2);

        let fb_indexes = fb.indexes.values().sorted_by_key(|i| &i.name).collect::<Vec<_>>();

        // btree([0,1]) sorts first alphabetically
        assert_eq!(
            fb_indexes[0].name,
            "fruit_basket_basket_id_fruit_name_idx_btree".into(),
            "btree index name uses canonical table and col names"
        );
        assert_eq!(
            &*fb_indexes[0].accessor_name, "RawBasketLookup",
            "btree index accessor_name is the raw source_name, never converted"
        );
        assert_eq!(
            fb_indexes[0].codegen_name,
            Some(id("fruit_basket_basket_id_fruit_name_idx_btree")),
            "codegen_name == autogenerated name in V10"
        );

        // direct(2) sorts second
        assert_eq!(
            fb_indexes[1].name,
            "fruit_basket_item_count_idx_direct".into(),
            "direct index name uses canonical table and col names"
        );
        assert_eq!(
            &*fb_indexes[1].accessor_name, "RawCountDirect",
            "direct index accessor_name is the raw source_name, never converted"
        );
        assert_eq!(
            fb_indexes[1].codegen_name,
            Some(id("fruit_basket_item_count_idx_direct")),
        );

        // deliveryRecord btree on col [2] "SeqId" → "seq_id"
        assert_eq!(dr.indexes.len(), 1);
        let dr_index = dr.indexes.values().next().unwrap();
        assert_eq!(
            dr_index.name,
            "delivery_record_seq_id_idx_btree".into(),
            "dr index name uses canonical table and col names"
        );
        assert_eq!(
            &*dr_index.accessor_name, "SeqIdIndex",
            "dr index accessor_name is the raw source_name, never converted"
        );

        // ═══════════════════════════════════════════════════════════════════════════
        // CONSTRAINT NAMES — autogenerated from canonical table + canonical col name
        // ═══════════════════════════════════════════════════════════════════════════
        //
        // unique on FruitBasket col [2] "ItemCount" → "item_count"
        //   → "fruit_basket_item_count_key"

        assert_eq!(fb.constraints.len(), 1);
        let (constraint_key, constraint) = fb.constraints.iter().next().unwrap();
        assert_eq!(
            &**constraint_key, "fruit_basket_item_count_key",
            "constraint name uses canonical table and col names"
        );
        assert_eq!(
            constraint.data,
            ConstraintData::Unique(UniqueConstraintData {
                columns: ColId(2).into()
            }),
        );

        // ═══════════════════════════════════════════════════════════════════════════
        // SEQUENCE NAMES — autogenerated from canonical table + canonical col name
        // ═══════════════════════════════════════════════════════════════════════════
        //
        // sequence on FruitBasket col [0] "BasketId" → "basket_id"
        //   → "fruit_basket_basket_id_seq"

        assert_eq!(fb.sequences.len(), 1);
        let (seq_key, _seq) = fb.sequences.iter().next().unwrap();
        assert_eq!(
            &**seq_key, "fruit_basket_basket_id_seq",
            "sequence name uses canonical table and col names"
        );

        // ═══════════════════════════════════════════════════════════════════════════
        // SCHEDULE NAMES — autogenerated from canonical table name
        // ═══════════════════════════════════════════════════════════════════════════
        //
        // "deliveryRecord" → "delivery_record" → "delivery_record_sched"

        let schedule = dr.schedule.as_ref().expect("deliveryRecord should have a schedule");
        assert_eq!(
            &*schedule.name, "delivery_record_sched",
            "schedule name uses canonical table name"
        );
        assert_eq!(
            schedule.function_name, do_delivery,
            "schedule function_name is the canonical reducer name"
        );
        assert_eq!(schedule.at_column, 1.into());
        assert_eq!(schedule.function_kind, FunctionKind::Reducer);
    }

    /// Tests that explicit name overrides bypass case-conversion policy,
    /// using the same schema as [`test_case_conversion_snake_case_policy`].
    ///
    /// Three overrides are applied on top of that schema:
    ///
    /// | Source name         | Kind     | Explicit canonical |
    /// |---------------------|----------|--------------------|
    /// | `"FruitBasket"`     | table    | `"FB"`             |
    /// | `"doDelivery"`      | function | `"Deliver"`        |
    /// | `"RawBasketLookup"` | index    | `"fb_lookuP"`      |
    ///
    /// Everything else is left to the default `SnakeCase` policy,
    /// proving overrides are scoped only to what was explicitly mapped.
    #[test]
    fn test_explicit_name_overrides() {
        use crate::def::*;
        use spacetimedb_lib::db::raw_def::v10::ExplicitNames;

        let id = |s: &str| Identifier::for_test(s);

        let (mut builder, _color_ref) = make_case_conversion_builder();

        let mut explicit = ExplicitNames::default();
        explicit.insert_table("FruitBasket", "FB"); // bypasses → "fruit_basket"
        explicit.insert_function("doDelivery", "Deliver"); // bypasses → "do_delivery"
        explicit.insert_index("RawBasketLookup", "fb_lookuP"); // bypasses autogenerated name
        builder.add_explicit_names(explicit);

        let def: ModuleDef = builder.finish().try_into().unwrap();

        // ═══════════════════════════════════════════════════════════════════════════
        // TABLE — explicit "FB" replaces policy-derived "fruit_basket"
        // ═══════════════════════════════════════════════════════════════════════════

        assert_eq!(def.tables.len(), 2);

        let fb_ident = id("FB");
        assert!(def.tables.contains_key(&fb_ident), "table 'FB' not found");
        assert!(
            !def.tables.contains_key(&id("fruit_basket")),
            "'fruit_basket' must not exist when overridden"
        );

        let fb = &def.tables[&fb_ident];
        assert_eq!(fb.name, fb_ident, "canonical name is the explicit value");
        assert_eq!(&*fb.accessor_name, "FruitBasket", "accessor_name preserves raw source");

        // Non-overridden table still follows SnakeCase.
        let delivery_record = id("delivery_record");
        assert!(def.tables.contains_key(&delivery_record));
        let dr = &def.tables[&delivery_record];
        assert_eq!(&*dr.accessor_name, "deliveryRecord");

        // ═══════════════════════════════════════════════════════════════════════════
        // COLUMNS — no explicit override; SnakeCase still applies
        // ═══════════════════════════════════════════════════════════════════════════

        assert_eq!(fb.columns[0].name, id("basket_id"), "col 0: SnakeCase unchanged");
        assert_eq!(fb.columns[1].name, id("fruit_name"), "col 1: SnakeCase unchanged");
        assert_eq!(fb.columns[2].name, id("item_count"), "col 2: SnakeCase unchanged");
        assert_eq!(fb.columns[3].name, id("color_label"), "col 3: SnakeCase unchanged");

        // ═══════════════════════════════════════════════════════════════════════════
        // INDEXES — one explicitly overridden, one not
        // ═══════════════════════════════════════════════════════════════════════════

        assert_eq!(fb.indexes.len(), 2);

        // "RawBasketLookup" → explicit "fb_lookuP"
        let idx_explicit = fb
            .indexes
            .values()
            .find(|i| &*i.accessor_name == "RawBasketLookup")
            .expect("index with accessor 'RawBasketLookup' not found");
        assert_eq!(
            idx_explicit.name,
            "fb_lookuP".into(),
            "explicit index name used verbatim"
        );
        assert_eq!(
            idx_explicit.codegen_name,
            Some(id("fb_lookuP")),
            "codegen_name matches explicit"
        );
        assert_eq!(
            &*idx_explicit.accessor_name, "RawBasketLookup",
            "accessor_name preserves raw source"
        );

        // "RawCountDirect" — no override; autogenerated from canonical table "FB" + col "item_count"
        let idx_auto = fb
            .indexes
            .values()
            .find(|i| &*i.accessor_name == "RawCountDirect")
            .expect("index with accessor 'RawCountDirect' not found");
        assert_eq!(
            idx_auto.name,
            "FB_item_count_idx_direct".into(),
            "non-overridden index autogenerated from explicit canonical table name"
        );
        assert_eq!(idx_auto.codegen_name, Some(id("FB_item_count_idx_direct")));
        assert_eq!(&*idx_auto.accessor_name, "RawCountDirect");

        // Non-overridden index on deliveryRecord still uses policy-derived table name.
        let dr_index = dr.indexes.values().next().unwrap();
        assert_eq!(dr_index.name, "delivery_record_seq_id_idx_btree".into());

        // ═══════════════════════════════════════════════════════════════════════════
        // AUTOGENERATED NAMES — all derived from explicit canonical table name "FB"
        // ═══════════════════════════════════════════════════════════════════════════

        // constraint: col [2] "ItemCount" → "item_count" under table "FB"
        //   → "FB_item_count_key"  (not "fruit_basket_item_count_key")
        assert_eq!(fb.constraints.len(), 1);
        let (constraint_key, constraint) = fb.constraints.iter().next().unwrap();
        assert_eq!(
            &**constraint_key, "FB_item_count_key",
            "constraint autogenerated from explicit canonical table name"
        );
        assert_eq!(
            constraint.data,
            ConstraintData::Unique(UniqueConstraintData {
                columns: ColId(2).into()
            }),
        );

        // sequence: col [0] "BasketId" → "basket_id" under table "FB"
        //   → "FB_basket_id_seq"  (not "fruit_basket_basket_id_seq")
        assert_eq!(fb.sequences.len(), 1);
        let (seq_key, _) = fb.sequences.iter().next().unwrap();
        assert_eq!(
            &**seq_key, "FB_basket_id_seq",
            "sequence autogenerated from explicit canonical table name"
        );

        // ═══════════════════════════════════════════════════════════════════════════
        // REDUCER — explicit "Deliver" replaces policy-derived "do_delivery"
        // ═══════════════════════════════════════════════════════════════════════════

        let deliver_ident = id("Deliver");
        assert!(def.reducers.contains_key(&deliver_ident), "reducer 'Deliver' not found");
        assert!(
            !def.reducers.contains_key(&id("do_delivery")),
            "'do_delivery' must not exist when overridden"
        );
        assert_eq!(def.reducers[&deliver_ident].name.as_identifier(), &deliver_ident);

        // Non-overridden reducer still follows SnakeCase.
        assert!(def.reducers.contains_key(&id("process_item")));
        assert!(!def.reducers.contains_key(&id("ProcessItem")));

        // ═══════════════════════════════════════════════════════════════════════════
        // SCHEDULE — function_name resolves to the explicit canonical reducer name
        // ═══════════════════════════════════════════════════════════════════════════

        let schedule = dr.schedule.as_ref().expect("deliveryRecord should have a schedule");
        assert_eq!(&*schedule.name, "delivery_record_sched");
        assert_eq!(
            schedule.function_name, deliver_ident,
            "schedule function_name uses the explicit canonical reducer name"
        );
        assert_eq!(schedule.at_column, 1.into());
        assert_eq!(schedule.function_kind, FunctionKind::Reducer);
    }
}
