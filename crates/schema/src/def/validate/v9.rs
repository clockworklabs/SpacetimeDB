use crate::def::*;
use crate::error::{RawColumnName, ValidationError};
use crate::type_for_generate::{ClientCodegenError, ProductTypeDef, TypespaceForGenerateBuilder};
use crate::{def::validate::Result, error::TypeLocation};
use spacetimedb_data_structures::error_stream::{CollectAllErrors, CombineErrors};
use spacetimedb_data_structures::map::HashSet;
use spacetimedb_lib::db::default_element_ordering::{product_type_has_default_ordering, sum_type_has_default_ordering};
use spacetimedb_lib::db::raw_def::v9::RawViewDefV9;
use spacetimedb_lib::ProductType;
use spacetimedb_primitives::col_list;
use spacetimedb_sats::{bsatn::de::Deserializer, de::DeserializeSeed, WithTypespace};
use std::borrow::Cow;

/// Validate a `RawModuleDefV9` and convert it into a `ModuleDef`,
/// or return a stream of errors if the definition is invalid.
pub fn validate(def: RawModuleDefV9) -> Result<ModuleDef> {
    let RawModuleDefV9 {
        typespace,
        tables,
        reducers,
        types,
        misc_exports,
        row_level_security,
    } = def;

    let known_type_definitions = types.iter().map(|def| def.ty);

    let mut validator = ModuleValidator {
        typespace: &typespace,
        stored_in_table_def: Default::default(),
        type_namespace: Default::default(),
        lifecycle_reducers: Default::default(),
        typespace_for_generate: TypespaceForGenerate::builder(&typespace, known_type_definitions),
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

    let reducers = reducers
        .into_iter()
        .enumerate()
        .map(|(idx, reducer)| {
            validator
                .validate_reducer_def(reducer, ReducerId(idx as u32))
                .map(|reducer_def| (reducer_def.name.clone(), reducer_def))
        })
        // Collect into a `Vec` first to preserve duplicate names.
        // Later on, in `check_function_names_are_unique`, we'll transform this into an `IndexMap`.
        .collect_all_errors::<Vec<_>>();

    let (procedures, misc_exports) =
        misc_exports
            .into_iter()
            .partition::<Vec<RawMiscModuleExportV9>, _>(|misc_export| {
                matches!(misc_export, RawMiscModuleExportV9::Procedure(_))
            });

    let (views, misc_exports) = misc_exports
        .into_iter()
        .partition::<Vec<RawMiscModuleExportV9>, _>(|misc_export| {
            matches!(misc_export, RawMiscModuleExportV9::View(_))
        });

    let procedures = procedures
        .into_iter()
        .map(|procedure| {
            let RawMiscModuleExportV9::Procedure(procedure) = procedure else {
                unreachable!("Already partitioned procedures separate from other `RawMiscModuleExportV9` variants");
            };
            procedure
        })
        .map(|procedure| {
            validator
                .validate_procedure_def(procedure)
                .map(|procedure_def| (procedure_def.name.clone(), procedure_def))
        })
        // Collect into a `Vec` first to preserve duplicate names.
        // Later on, in `check_function_names_are_unique`, we'll transform this into an `IndexMap`.
        .collect_all_errors::<Vec<_>>();

    let views = views
        .into_iter()
        .map(|view| {
            let RawMiscModuleExportV9::View(view) = view else {
                unreachable!("Already partitioned views separate from other `RawMiscModuleExportV9` variants");
            };
            view
        })
        .map(|view| {
            validator
                .validate_view_def(view)
                .map(|view_def| (view_def.name.clone(), view_def))
        })
        .collect_all_errors();

    let tables = tables
        .into_iter()
        .map(|table| {
            validator
                .validate_table_def(table)
                .map(|table_def| (table_def.name.clone(), table_def))
        })
        .collect_all_errors();

    let row_level_security_raw = row_level_security
        .into_iter()
        .map(|rls| (rls.sql.clone(), rls))
        .collect();

    let mut refmap = HashMap::default();
    let types = types
        .into_iter()
        .map(|ty| {
            validator.validate_type_def(ty).map(|type_def| {
                refmap.insert(type_def.ty, type_def.name.clone());
                (type_def.name.clone(), type_def)
            })
        })
        .collect_all_errors::<HashMap<_, _>>();

    let tables_types_reducers_procedures_views = (tables, types, reducers, procedures, views)
        .combine_errors()
        .and_then(|(mut tables, types, reducers, procedures, views)| {
            let ((reducers, procedures, views), ()) = (
                check_function_names_are_unique(reducers, procedures, views),
                check_non_procedure_misc_exports(misc_exports, &validator, &mut tables),
            )
                .combine_errors()?;
            check_scheduled_functions_exist(&mut tables, &reducers, &procedures)?;
            Ok((tables, types, reducers, procedures, views))
        });

    let ModuleValidator {
        stored_in_table_def,
        typespace_for_generate,
        lifecycle_reducers,
        ..
    } = validator;

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
    })
}

/// Collects state used during validation.
struct ModuleValidator<'a> {
    /// The typespace of the module.
    ///
    /// Behind a reference to ensure we don't accidentally mutate it.
    typespace: &'a Typespace,

    /// The in-progress typespace used to generate client types.
    typespace_for_generate: TypespaceForGenerateBuilder<'a>,

    /// Names we have seen so far.
    ///
    /// It would be nice if we could have span information here, but currently it isn't passed
    /// through the ABI boundary.
    /// We could add it as a `MiscModuleExport` later without breaking the ABI.
    stored_in_table_def: StrMap<Identifier>,

    /// Module-scoped type names we have seen so far.
    type_namespace: HashMap<ScopedTypeName, AlgebraicTypeRef>,

    /// Reducers that play special lifecycle roles.
    lifecycle_reducers: EnumMap<Lifecycle, Option<ReducerId>>,
}

impl ModuleValidator<'_> {
    fn validate_table_def(&mut self, table: RawTableDefV9) -> Result<TableDef> {
        let RawTableDefV9 {
            name: raw_table_name,
            product_type_ref,
            primary_key,
            indexes,
            constraints,
            sequences,
            schedule,
            table_type,
            table_access,
        } = table;

        // We exit early if we don't find the product type ref,
        // since this breaks all the other checks.
        let product_type: &ProductType = self
            .typespace
            .get(product_type_ref)
            .and_then(AlgebraicType::as_product)
            .ok_or_else(|| {
                ValidationErrors::from(ValidationError::InvalidProductTypeRef {
                    table: raw_table_name.clone(),
                    ref_: product_type_ref,
                })
            })?;

        let mut table_in_progress = TableValidator {
            raw_name: raw_table_name.clone(),
            product_type_ref,
            product_type,
            module_validator: self,
            has_sequence: Default::default(),
        };

        let columns = (0..product_type.elements.len())
            .map(|id| table_in_progress.validate_column_def(id.into()))
            .collect_all_errors();

        let indexes = indexes
            .into_iter()
            .map(|index| {
                table_in_progress
                    .validate_index_def(index)
                    .map(|index| (index.name.clone(), index))
            })
            .collect_all_errors::<StrMap<_>>();

        // We can't validate the primary key without validating the unique constraints first.
        let primary_key_head = primary_key.head();
        let constraints_primary_key = constraints
            .into_iter()
            .map(|constraint| {
                table_in_progress
                    .validate_constraint_def(constraint)
                    .map(|constraint| (constraint.name.clone(), constraint))
            })
            .collect_all_errors()
            .and_then(|constraints: StrMap<ConstraintDef>| {
                table_in_progress.validate_primary_key(constraints, primary_key)
            });

        // Now that we've validated indices and constraints separately,
        // we can validate their interactions.
        // More specifically, a direct index requires a unique constraint.
        let constraints_backed_by_indices =
            if let (Ok((constraints, _)), Ok(indexes)) = (&constraints_primary_key, &indexes) {
                constraints
                    .values()
                    .filter_map(|c| c.data.unique_columns().map(|cols| (c, cols)))
                    // TODO(centril): this check is actually too strict
                    // and ends up unnecessarily inducing extra indices.
                    //
                    // It is sufficient for `unique_cols` to:
                    // a) be a permutation of `index`'s columns,
                    //    as a permutation of a set is still the same set,
                    //    so when we use the index to check the constraint,
                    //    the order in the index does not matter for the purposes of the constraint.
                    //
                    // b) for `unique_cols` to form a prefix of `index`'s columns,
                    //    if the index provides efficient prefix scans.
                    //
                    // Currently, b) is unsupported,
                    // as we cannot decouple unique constraints from indices in the datastore today,
                    // and we cannot mark the entire index unique,
                    // as that would not be a sound representation of what the user wanted.
                    // If we wanted to, we could make the constraints merely use indices,
                    // rather than be indices.
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
                table_in_progress
                    .validate_sequence_def(sequence)
                    .map(|sequence| (sequence.name.clone(), sequence))
            })
            .collect_all_errors();

        let schedule = schedule
            .map(|schedule| table_in_progress.validate_schedule_def(schedule, primary_key_head))
            .transpose();

        let name = table_in_progress
            .add_to_global_namespace(raw_table_name.clone())
            .and_then(|name| {
                let name = identifier(name)?;
                if table_type != TableType::System && name.starts_with("st_") {
                    Err(ValidationError::TableNameReserved { table: name }.into())
                } else {
                    Ok(name)
                }
            });

        let (name, columns, indexes, (constraints, primary_key), (), sequences, schedule) = (
            name,
            columns,
            indexes,
            constraints_primary_key,
            constraints_backed_by_indices,
            sequences,
            schedule,
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
            schedule,
            table_type,
            table_access,
        })
    }

    fn params_for_generate<'a>(
        &mut self,
        params: &'a ProductType,
        make_type_location: impl Fn(usize, Option<Cow<'a, str>>) -> TypeLocation<'a>,
    ) -> Result<Box<[(Identifier, AlgebraicTypeUse)]>> {
        params
            .elements
            .iter()
            .enumerate()
            .map(|(position, param)| {
                // Note: this does not allocate, since `TypeLocation` is defined using `Cow`.
                // We only allocate if an error is returned.
                let location = make_type_location(position, param.name().map(Into::into));
                let param_name = param
                    .name()
                    .ok_or_else(|| {
                        ValidationError::ClientCodegenError {
                            location: location.clone().make_static(),
                            error: ClientCodegenError::NamelessReducerParam,
                        }
                        .into()
                    })
                    .and_then(|s| identifier(s.into()));
                let ty_use = self.validate_for_type_use(&location, &param.algebraic_type);
                (param_name, ty_use).combine_errors()
            })
            .collect_all_errors()
    }

    /// Validate a reducer definition.
    fn validate_reducer_def(&mut self, reducer_def: RawReducerDefV9, reducer_id: ReducerId) -> Result<ReducerDef> {
        let RawReducerDefV9 {
            name,
            params,
            lifecycle,
        } = reducer_def;

        let params_for_generate: Result<_> =
            self.params_for_generate(&params, |position, arg_name| TypeLocation::ReducerArg {
                reducer_name: (&*name).into(),
                position,
                arg_name,
            });

        // Reducers share the "function namespace" with procedures.
        // Uniqueness is validated in a later pass, in `check_function_names_are_unique`.
        let name = identifier(name.clone());

        let lifecycle = lifecycle
            .map(|lifecycle| match &mut self.lifecycle_reducers[lifecycle] {
                x @ None => {
                    *x = Some(reducer_id);
                    Ok(lifecycle)
                }
                Some(_) => Err(ValidationError::DuplicateLifecycle { lifecycle }.into()),
            })
            .transpose();
        let (name, params_for_generate, lifecycle) = (name, params_for_generate, lifecycle).combine_errors()?;
        Ok(ReducerDef {
            name,
            params: params.clone(),
            params_for_generate: ProductTypeDef {
                elements: params_for_generate,
                recursive: false, // A ProductTypeDef not stored in a Typespace cannot be recursive.
            },
            lifecycle,
        })
    }

    fn validate_procedure_def(&mut self, procedure_def: RawProcedureDefV9) -> Result<ProcedureDef> {
        let RawProcedureDefV9 {
            name,
            params,
            return_type,
        } = procedure_def;

        let params_for_generate = self.params_for_generate(&params, |position, arg_name| TypeLocation::ProcedureArg {
            procedure_name: Cow::Borrowed(&name),
            position,
            arg_name,
        });

        let return_type_for_generate = self.validate_for_type_use(
            &TypeLocation::ProcedureReturn {
                procedure_name: Cow::Borrowed(&name),
            },
            &return_type,
        );

        // Procedures share the "function namespace" with reducers.
        // Uniqueness is validated in a later pass, in `check_function_names_are_unique`.
        let name = identifier(name);

        let (name, params_for_generate, return_type_for_generate) =
            (name, params_for_generate, return_type_for_generate).combine_errors()?;

        Ok(ProcedureDef {
            name,
            params,
            params_for_generate: ProductTypeDef {
                elements: params_for_generate,
                recursive: false, // A ProductTypeDef not stored in a Typespace cannot be recursive.
            },
            return_type,
            return_type_for_generate,
        })
    }

    /// Validate a view definition.
    fn validate_view_def(&mut self, view_def: RawViewDefV9) -> Result<ViewDef> {
        let RawViewDefV9 {
            name,
            is_public,
            is_anonymous,
            params,
            return_type,
        } = view_def;

        let invalid_return_type = || {
            ValidationErrors::from(ValidationError::InvalidViewReturnType {
                view: name.clone(),
                ty: return_type.clone().into(),
            })
        };

        // The possible return types of a view are `Vec<T>` or `Option<T>`,
        // where `T` is a `ProductType` in the `Typespace`.
        // Here we extract the inner product type ref `T`.
        // We exit early for errors since this breaks all the other checks.
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
            .typespace
            .get(product_type_ref)
            .and_then(AlgebraicType::as_product)
            .ok_or_else(|| {
                ValidationErrors::from(ValidationError::InvalidProductTypeRef {
                    table: name.clone(),
                    ref_: product_type_ref,
                })
            })?;

        let params_for_generate = self.params_for_generate(&params, |position, arg_name| TypeLocation::ViewArg {
            view_name: Cow::Borrowed(&name),
            position,
            arg_name,
        })?;

        let return_type_for_generate = self.validate_for_type_use(
            &TypeLocation::ViewReturn {
                view_name: Cow::Borrowed(&name),
            },
            &return_type,
        );

        let mut view_in_progress = ViewValidator::new(
            name.clone(),
            product_type_ref,
            product_type,
            &params,
            &params_for_generate,
            self,
        );

        // Views have the same interface as tables and therefore must be registered in the global namespace.
        //
        // Note, views also share the "function namespace" with reducers and procedures.
        // While this isn't strictly necessary because reducers and views have different calling contexts,
        // we may want to support calling views in the same context as reducers in the future (e.g. `spacetime call`).
        // Hence we validate uniqueness among reducer, procedure, and view names in a later pass.
        // See `check_function_names_are_unique`.
        let name = view_in_progress.add_to_global_namespace(name).and_then(identifier);

        let n = product_type.elements.len();
        let return_columns = (0..n)
            .map(|id| view_in_progress.validate_view_column_def(id.into()))
            .collect_all_errors();

        let n = params.elements.len();
        let param_columns = (0..n)
            .map(|id| view_in_progress.validate_param_column_def(id.into()))
            .collect_all_errors();

        let (name, return_type_for_generate, return_columns, param_columns) =
            (name, return_type_for_generate, return_columns, param_columns).combine_errors()?;

        Ok(ViewDef {
            name,
            is_anonymous,
            is_public,
            params,
            params_for_generate: ProductTypeDef {
                elements: params_for_generate,
                recursive: false, // A `ProductTypeDef` not stored in a `Typespace` cannot be recursive.
            },
            return_type,
            return_type_for_generate,
            return_columns,
            param_columns,
        })
    }

    fn validate_column_default_value(
        &self,
        tables: &HashMap<Identifier, TableDef>,
        cdv: &RawColumnDefaultValueV9,
    ) -> Result<AlgebraicValue> {
        let table_name = identifier(cdv.table.clone())?;

        // Extract the table. We cannot make progress otherwise.
        let table = tables.get(&table_name).ok_or_else(|| ValidationError::TableNotFound {
            table: cdv.table.clone(),
        })?;

        // Get the column that a default is being added to.
        let Some(col) = table.columns.get(cdv.col_id.idx()) else {
            return Err(ValidationError::ColumnNotFound {
                table: cdv.table.clone(),
                def: cdv.table.clone(),
                column: cdv.col_id,
            }
            .into());
        };

        // First time the type of the default value is known, so decode it.
        let mut reader = &cdv.value[..];
        let ty = WithTypespace::new(self.typespace, &col.ty);
        let field_value: Result<AlgebraicValue> =
            ty.deserialize(Deserializer::new(&mut reader)).map_err(|decode_error| {
                ValidationError::ColumnDefaultValueMalformed {
                    table: cdv.table.clone(),
                    col_id: cdv.col_id,
                    err: decode_error,
                }
                .into()
            });

        field_value
    }

    /// Validate a type definition.
    fn validate_type_def(&mut self, type_def: RawTypeDefV9) -> Result<TypeDef> {
        let RawTypeDefV9 {
            name,
            ty,
            custom_ordering,
        } = type_def;

        // Do these together since they are related.
        let ty_custom_ordering: Result<(AlgebraicTypeRef, bool)> = self
            .typespace
            .get(ty)
            .ok_or_else(|| {
                ValidationError::InvalidTypeRef {
                    type_name: name.clone(),
                    ref_: ty,
                }
                .into()
            })
            .and_then(|pointed_to| {
                let ordering_ok = if custom_ordering {
                    Ok(())
                } else {
                    let correct = match pointed_to {
                        AlgebraicType::Sum(sum) => sum_type_has_default_ordering(sum),
                        AlgebraicType::Product(product) => product_type_has_default_ordering(product),
                        _ => true,
                    };
                    if correct {
                        Ok(())
                    } else {
                        Err(ValidationError::TypeHasIncorrectOrdering {
                            type_name: name.clone(),
                            ref_: ty,
                            bad_type: pointed_to.clone().into(),
                        }
                        .into())
                    }
                };

                // Now check the definition is valid
                let def_ok = self.validate_for_type_definition(ty);

                let ((), ()) = (ordering_ok, def_ok).combine_errors()?;

                // note: we return the reference `ty`, not the pointed-to type `pointed_to`.
                // The reference is semantically important.
                Ok((ty, custom_ordering))
            });

        let RawScopedTypeNameV9 {
            name: unscoped_name,
            scope,
        } = name;
        let unscoped_name = identifier(unscoped_name);
        let scope = Vec::from(scope).into_iter().map(identifier).collect_all_errors();
        let name = (unscoped_name, scope)
            .combine_errors()
            .and_then(|(unscoped_name, scope)| {
                let result = ScopedTypeName {
                    name: unscoped_name,
                    scope,
                };
                match self.type_namespace.insert(result.clone(), ty) {
                    Some(_) => Err(ValidationError::DuplicateTypeName { name: result.clone() }.into()),
                    None => Ok(result),
                }
            });

        let (name, (ty, custom_ordering)) = (name, ty_custom_ordering).combine_errors()?;

        Ok(TypeDef {
            name,
            ty,
            custom_ordering,
        })
    }

    /// Validates that a type can be used to generate a client type use.
    fn validate_for_type_use(&mut self, location: &TypeLocation, ty: &AlgebraicType) -> Result<AlgebraicTypeUse> {
        self.typespace_for_generate.parse_use(ty).map_err(|err| {
            ErrorStream::expect_nonempty(err.into_iter().map(|error| ValidationError::ClientCodegenError {
                location: location.clone().make_static(),
                error,
            }))
        })
    }

    /// Validates that a type can be used to generate a client type definition.
    fn validate_for_type_definition(&mut self, ref_: AlgebraicTypeRef) -> Result<()> {
        self.typespace_for_generate.add_definition(ref_).map_err(|err| {
            ErrorStream::expect_nonempty(err.into_iter().map(|error| ValidationError::ClientCodegenError {
                location: TypeLocation::InTypespace { ref_ },
                error,
            }))
        })
    }
}

/// A partially validated view.
///
/// This is just a small wrapper around [`TableValidator`] so that we can:
/// 1. Validate column defs
/// 2. Insert view names into the global namespace.
struct ViewValidator<'a, 'b> {
    inner: TableValidator<'a, 'b>,
    params: &'a ProductType,
    params_for_generate: &'a [(Identifier, AlgebraicTypeUse)],
}

impl<'a, 'b> ViewValidator<'a, 'b> {
    fn new(
        raw_name: Box<str>,
        product_type_ref: AlgebraicTypeRef,
        product_type: &'a ProductType,
        params: &'a ProductType,
        params_for_generate: &'a [(Identifier, AlgebraicTypeUse)],
        module_validator: &'a mut ModuleValidator<'b>,
    ) -> Self {
        Self {
            inner: TableValidator {
                raw_name,
                product_type_ref,
                product_type,
                module_validator,
                has_sequence: Default::default(),
            },
            params,
            params_for_generate,
        }
    }

    fn validate_param_column_def(&mut self, col_id: ColId) -> Result<ViewParamDef> {
        let column = &self
            .params
            .elements
            .get(col_id.idx())
            .expect("enumerate is generating an out-of-range index...");

        let (_, ty_for_generate) = self
            .params_for_generate
            .get(col_id.idx())
            .expect("enumerate is generating an out-of-range index...");

        let name: Result<Identifier> = identifier(
            column
                .name()
                .map(|name| name.into())
                .unwrap_or_else(|| format!("param_{}", col_id).into_boxed_str()),
        );

        // This error will be created multiple times if the view name is invalid,
        // but we sort and deduplicate the error stream afterwards,
        // so it isn't a huge deal.
        //
        // This is necessary because we require `ErrorStream` to be nonempty.
        // We need to put something in there if the view name is invalid.
        let view_name = identifier(self.inner.raw_name.clone());

        let (name, view_name) = (name, view_name).combine_errors()?;

        Ok(ViewParamDef {
            name,
            ty: column.algebraic_type.clone(),
            ty_for_generate: ty_for_generate.clone(),
            col_id,
            view_name,
        })
    }

    fn validate_view_column_def(&mut self, col_id: ColId) -> Result<ViewColumnDef> {
        self.inner.validate_column_def(col_id).map(ViewColumnDef::from)
    }

    fn add_to_global_namespace(&mut self, name: Box<str>) -> Result<Box<str>> {
        self.inner.add_to_global_namespace(name)
    }
}

/// A partially validated table.
struct TableValidator<'a, 'b> {
    module_validator: &'a mut ModuleValidator<'b>,
    raw_name: Box<str>,
    product_type_ref: AlgebraicTypeRef,
    product_type: &'a ProductType,
    has_sequence: HashSet<ColId>,
}

impl TableValidator<'_, '_> {
    /// Validate a column.
    ///
    /// Note that this accepts a `ProductTypeElement` rather than a `ColumnDef`,
    /// because all information about columns is stored in the `Typespace` in ABI version 9.
    fn validate_column_def(&mut self, col_id: ColId) -> Result<ColumnDef> {
        let column = &self
            .product_type
            .elements
            .get(col_id.idx())
            .expect("enumerate is generating an out-of-range index...");

        let name: Result<Identifier> = column
            .name()
            .ok_or_else(|| {
                ValidationError::UnnamedColumn {
                    column: self.raw_column_name(col_id),
                }
                .into()
            })
            .and_then(|name| identifier(name.into()));

        let ty_for_generate = self.module_validator.validate_for_type_use(
            &TypeLocation::InTypespace {
                ref_: self.product_type_ref,
            },
            &column.algebraic_type,
        );

        // This error will be created multiple times if the table name is invalid,
        // but we sort and deduplicate the error stream afterwards,
        // so it isn't a huge deal.
        //
        // This is necessary because we require `ErrorStream` to be
        // nonempty. We need to put something in there if the table name is invalid.
        let table_name = identifier(self.raw_name.clone());

        let (name, ty_for_generate, table_name) = (name, ty_for_generate, table_name).combine_errors()?;

        Ok(ColumnDef {
            name,
            ty: column.algebraic_type.clone(),
            ty_for_generate,
            col_id,
            table_name,
            default_value: None, // filled in later
        })
    }

    fn validate_primary_key(
        &mut self,
        validated_constraints: StrMap<ConstraintDef>,
        primary_key: ColList,
    ) -> Result<(StrMap<ConstraintDef>, Option<ColId>)> {
        if primary_key.len() > 1 {
            return Err(ValidationError::RepeatedPrimaryKey {
                table: self.raw_name.clone(),
            }
            .into());
        }
        let pk = primary_key
            .head()
            .map(|pk| -> Result<ColId> {
                let pk = self.validate_col_id(&self.raw_name, pk)?;
                let pk_col_list = ColSet::from(pk);
                if validated_constraints.values().any(|constraint| {
                    let ConstraintData::Unique(UniqueConstraintData { columns }) = &constraint.data;
                    columns == &pk_col_list
                }) {
                    Ok(pk)
                } else {
                    Err(ValidationError::MissingPrimaryKeyUniqueConstraint {
                        column: self.raw_column_name(pk),
                    }
                    .into())
                }
            })
            .transpose()?;
        Ok((validated_constraints, pk))
    }

    fn validate_sequence_def(&mut self, sequence: RawSequenceDefV9) -> Result<SequenceDef> {
        let RawSequenceDefV9 {
            column,
            min_value,
            start,
            max_value,
            increment,
            name,
        } = sequence;

        let name = name.unwrap_or_else(|| generate_sequence_name(&self.raw_name, self.product_type, column));

        // The column for the sequence exists and is an appropriate type.
        let column = self.validate_col_id(&name, column).and_then(|col_id| {
            let ty = &self.product_type.elements[col_id.idx()].algebraic_type;

            if !ty.is_integer() {
                Err(ValidationError::InvalidSequenceColumnType {
                    sequence: name.clone(),
                    column: self.raw_column_name(col_id),
                    column_type: ty.clone().into(),
                }
                .into())
            } else if !self.has_sequence.insert(col_id) {
                Err(ValidationError::OneAutoInc {
                    column: self.raw_column_name(col_id),
                }
                .into())
            } else {
                Ok(col_id)
            }
        });

        /// Compare two `Option<i128>` values, returning `true` if `lo <= hi`,
        /// or if either is `None`.
        fn le(lo: Option<i128>, hi: Option<i128>) -> bool {
            match (lo, hi) {
                (Some(lo), Some(hi)) => lo <= hi,
                _ => true,
            }
        }
        let valid = le(min_value, start) && le(start, max_value) && le(min_value, max_value);

        let min_start_max = if valid {
            Ok((min_value, start, max_value))
        } else {
            Err(ValidationError::InvalidSequenceRange {
                sequence: name.clone(),
                min_value,
                start,
                max_value,
            }
            .into())
        };

        let name = self.add_to_global_namespace(name);

        let (name, column, (min_value, start, max_value)) = (name, column, min_start_max).combine_errors()?;

        Ok(SequenceDef {
            name,
            column,
            min_value,
            start,
            max_value,
            increment,
        })
    }

    /// Validate an index definition.
    fn validate_index_def(&mut self, index: RawIndexDefV9) -> Result<IndexDef> {
        let RawIndexDefV9 {
            name,
            algorithm,
            accessor_name,
        } = index;

        let name = name.unwrap_or_else(|| generate_index_name(&self.raw_name, self.product_type, &algorithm));

        let algorithm: Result<IndexAlgorithm> = match algorithm {
            RawIndexAlgorithm::BTree { columns } => self
                .validate_col_ids(&name, columns)
                .map(|columns| BTreeAlgorithm { columns }.into()),
            RawIndexAlgorithm::Direct { column } => self.validate_col_id(&name, column).and_then(|column| {
                let field = &self.product_type.elements[column.idx()];
                let ty = &field.algebraic_type;
                let is_bad_type = match ty {
                    AlgebraicType::U8 | AlgebraicType::U16 | AlgebraicType::U32 | AlgebraicType::U64 => false,
                    AlgebraicType::Ref(r) => self.module_validator.typespace[*r]
                        .as_sum()
                        .is_none_or(|s| !s.is_simple_enum()),
                    AlgebraicType::Sum(sum) if sum.is_simple_enum() => false,
                    _ => true,
                };
                if is_bad_type {
                    return Err(ValidationError::DirectIndexOnBadType {
                        index: name.clone(),
                        column: field.name.clone().unwrap_or_else(|| column.idx().to_string().into()),
                        ty: ty.clone().into(),
                    }
                    .into());
                }
                Ok(DirectAlgorithm { column }.into())
            }),
            _ => Err(ValidationError::HashIndexUnsupported { index: name.clone() }.into()),
        };
        let name = self.add_to_global_namespace(name);
        let accessor_name = accessor_name.map(identifier).transpose();

        let (name, accessor_name, algorithm) = (name, accessor_name, algorithm).combine_errors()?;

        Ok(IndexDef {
            name,
            algorithm,
            accessor_name,
        })
    }

    /// Validate a unique constraint definition.
    fn validate_constraint_def(&mut self, constraint: RawConstraintDefV9) -> Result<ConstraintDef> {
        let RawConstraintDefV9 { name, data } = constraint;

        if let RawConstraintDataV9::Unique(RawUniqueConstraintDataV9 { columns }) = data {
            let name =
                name.unwrap_or_else(|| generate_unique_constraint_name(&self.raw_name, self.product_type, &columns));

            let columns: Result<ColList> = self.validate_col_ids(&name, columns);
            let name = self.add_to_global_namespace(name);

            let (name, columns) = (name, columns).combine_errors()?;
            let columns: ColSet = columns.into();
            Ok(ConstraintDef {
                name,
                data: ConstraintData::Unique(UniqueConstraintData { columns }),
            })
        } else {
            unimplemented!("Unknown constraint type")
        }
    }

    /// Validate a schedule definition.
    fn validate_schedule_def(&mut self, schedule: RawScheduleDefV9, primary_key: Option<ColId>) -> Result<ScheduleDef> {
        let RawScheduleDefV9 {
            // Despite the field name, a `RawScheduleDefV9` may refer to either a reducer or a function.
            reducer_name: function_name,
            scheduled_at_column,
            name,
        } = schedule;

        let name = name.unwrap_or_else(|| generate_schedule_name(&self.raw_name));

        // Find the appropriate columns.
        let at_column = self
            .product_type
            .elements
            .get(scheduled_at_column.idx())
            .is_some_and(|ty| ty.algebraic_type.is_schedule_at())
            .then_some(scheduled_at_column);

        let id_column = primary_key.filter(|pk| {
            self.product_type
                .elements
                .get(pk.idx())
                .is_some_and(|ty| ty.algebraic_type == AlgebraicType::U64)
        });

        // Error if either column is missing.
        let at_id = at_column.zip(id_column).ok_or_else(|| {
            ValidationError::ScheduledIncorrectColumns {
                table: self.raw_name.clone(),
                columns: self.product_type.clone(),
            }
            .into()
        });

        let name = self.add_to_global_namespace(name);
        let function_name = identifier(function_name);

        let (name, (at_column, id_column), function_name) = (name, at_id, function_name).combine_errors()?;

        Ok(ScheduleDef {
            name,
            at_column,
            id_column,
            function_name,

            // Fill this in as a placeholder now.
            // It will be populated with the correct `FunctionKind` later,
            // in `check_scheduled_functions_exist`.
            function_kind: FunctionKind::Unknown,
        })
    }

    /// Validate `name` as an `Identifier` and add it to the global namespace, registering the corresponding `Def` as being stored in a  particular `TableDef`.
    ///
    /// If it has already been added, return an error.
    ///
    /// This is not used for all `Def` types.
    fn add_to_global_namespace(&mut self, name: Box<str>) -> Result<Box<str>> {
        let table_name = identifier(self.raw_name.clone())?;

        // This may report the table_name as invalid multiple times, but this will be removed
        // when we sort and deduplicate the error stream.
        if self.module_validator.stored_in_table_def.contains_key(&name) {
            Err(ValidationError::DuplicateName { name }.into())
        } else {
            self.module_validator
                .stored_in_table_def
                .insert(name.clone(), table_name);
            Ok(name)
        }
    }

    /// Validate a `ColId` for this table, returning it unmodified if valid.
    /// `def_name` is the name of the definition being validated and is used in errors.
    pub fn validate_col_id(&self, def_name: &str, col_id: ColId) -> Result<ColId> {
        if self.product_type.elements.get(col_id.idx()).is_some() {
            Ok(col_id)
        } else {
            Err(ValidationError::ColumnNotFound {
                column: col_id,
                table: self.raw_name.clone(),
                def: def_name.into(),
            }
            .into())
        }
    }

    /// Validate a `ColList` for this table, returning it unmodified if valid.
    /// `def_name` is the name of the definition being validated and is used in errors.
    pub fn validate_col_ids(&self, def_name: &str, ids: ColList) -> Result<ColList> {
        let mut collected: Vec<ColId> = ids
            .iter()
            .map(|column| self.validate_col_id(def_name, column))
            .collect_all_errors()?;

        collected.sort();
        collected.dedup();

        if collected.len() != ids.len() as usize {
            Err(ValidationError::DuplicateColumns {
                columns: ids,
                def: def_name.into(),
            }
            .into())
        } else {
            Ok(ids)
        }
    }

    /// Return a best effort name for this column, to be used in errors.
    /// If we can't find a string name for it, use an integer instead.
    ///
    /// (It's generally preferable to avoid integer names, since types using the default
    /// ordering are implicitly shuffled!)
    pub fn raw_column_name(&self, col_id: ColId) -> RawColumnName {
        let column: Box<str> = self
            .product_type
            .elements
            .get(col_id.idx())
            .and_then(|col| col.name())
            .map(|name| name.into())
            .unwrap_or_else(|| format!("{col_id}").into());

        RawColumnName {
            table: self.raw_name.clone(),
            column,
        }
    }
}

/// Get the name of a column in the typespace.
///
/// Only used for generating names for indexes, sequences, and unique constraints.
///
/// Generates `col_{column}` if the column has no name or if the `RawTableDef`'s `table_type_ref`
/// was initialized incorrectly.
fn column_name(table_type: &ProductType, column: ColId) -> String {
    table_type
        .elements
        .get(column.idx())
        .and_then(|column| column.name().map(ToString::to_string))
        .unwrap_or_else(|| format!("col_{}", column.0))
}

/// Concatenate a list of column names.
fn concat_column_names(table_type: &ProductType, selected: &ColList) -> String {
    selected.iter().map(|col| column_name(table_type, col)).join("_")
}

/// All indexes have this name format.
pub fn generate_index_name(table_name: &str, table_type: &ProductType, algorithm: &RawIndexAlgorithm) -> RawIdentifier {
    let (label, columns) = match algorithm {
        RawIndexAlgorithm::BTree { columns } => ("btree", columns),
        RawIndexAlgorithm::Direct { column } => ("direct", &col_list![*column]),
        RawIndexAlgorithm::Hash { columns } => ("hash", columns),
        _ => unimplemented!("Unknown index algorithm {:?}", algorithm),
    };
    let column_names = concat_column_names(table_type, columns);
    format!("{table_name}_{column_names}_idx_{label}").into()
}

/// All sequences have this name format.
pub fn generate_sequence_name(table_name: &str, table_type: &ProductType, column: ColId) -> RawIdentifier {
    let column_name = column_name(table_type, column);
    format!("{table_name}_{column_name}_seq").into()
}

/// All schedules have this name format.
pub fn generate_schedule_name(table_name: &str) -> RawIdentifier {
    format!("{table_name}_sched").into()
}

/// All unique constraints have this name format.
pub fn generate_unique_constraint_name(
    table_name: &str,
    product_type: &ProductType,
    columns: &ColList,
) -> RawIdentifier {
    let column_names = concat_column_names(product_type, columns);
    format!("{table_name}_{column_names}_key").into()
}

/// Helper to create an `Identifier` from a `str` with the appropriate error type.
/// TODO: memoize this.
fn identifier(name: Box<str>) -> Result<Identifier> {
    Identifier::new(name).map_err(|error| ValidationError::IdentifierError { error }.into())
}

/// Check that every [`ScheduleDef`]'s `function_name` refers to a real reducer or procedure
/// and that the function's arguments are appropriate for the table,
/// then record the scheduled function's [`FunctionKind`] in the [`ScheduleDef`].
fn check_scheduled_functions_exist(
    tables: &mut IdentifierMap<TableDef>,
    reducers: &IndexMap<Identifier, ReducerDef>,
    procedures: &IndexMap<Identifier, ProcedureDef>,
) -> Result<()> {
    let validate_params =
        |params_from_function: &ProductType, table_row_type_ref: AlgebraicTypeRef, function_name: &str| {
            if params_from_function.elements.len() == 1
                && params_from_function.elements[0].algebraic_type == table_row_type_ref.into()
            {
                Ok(())
            } else {
                Err(ValidationError::IncorrectScheduledFunctionParams {
                    function_name: function_name.into(),
                    function_kind: FunctionKind::Reducer,
                    expected: AlgebraicType::product([AlgebraicType::Ref(table_row_type_ref)]).into(),
                    actual: params_from_function.clone().into(),
                })
            }
        };

    tables
        .values_mut()
        .map(|table| -> Result<()> {
            if let Some(schedule) = &mut table.schedule {
                if let Some(reducer) = reducers.get(&schedule.function_name) {
                    schedule.function_kind = FunctionKind::Reducer;
                    validate_params(&reducer.params, table.product_type_ref, &reducer.name).map_err(Into::into)
                } else if let Some(procedure) = procedures.get(&schedule.function_name) {
                    schedule.function_kind = FunctionKind::Procedure;
                    validate_params(&procedure.params, table.product_type_ref, &procedure.name).map_err(Into::into)
                } else {
                    Err(ValidationError::MissingScheduledFunction {
                        schedule: schedule.name.clone(),
                        function: schedule.function_name.clone(),
                    }
                    .into())
                }
            } else {
                Ok(())
            }
        })
        .collect_all_errors()
}

/// Check that all function (reducer, procedure, or view) names are unique,
/// then re-organize the reducers and procedures into [`IndexMap`]s
/// for storage in the [`ModuleDef`].
#[allow(clippy::type_complexity)]
fn check_function_names_are_unique(
    reducers: Vec<(Identifier, ReducerDef)>,
    procedures: Vec<(Identifier, ProcedureDef)>,
    views: Vec<(Identifier, ViewDef)>,
) -> Result<(
    IndexMap<Identifier, ReducerDef>,
    IndexMap<Identifier, ProcedureDef>,
    IndexMap<Identifier, ViewDef>,
)> {
    let mut errors = vec![];

    let mut reducers_map = IndexMap::with_capacity(reducers.len());

    for (name, def) in reducers {
        if reducers_map.contains_key(&name) {
            errors.push(ValidationError::DuplicateFunctionName { name });
        } else {
            reducers_map.insert(name, def);
        }
    }

    let mut procedures_map = IndexMap::with_capacity(procedures.len());

    for (name, def) in procedures {
        if reducers_map.contains_key(&name) || procedures_map.contains_key(&name) {
            errors.push(ValidationError::DuplicateFunctionName { name });
        } else {
            procedures_map.insert(name, def);
        }
    }

    let mut views_map = IndexMap::with_capacity(views.len());

    for (name, def) in views {
        if reducers_map.contains_key(&name) || procedures_map.contains_key(&name) || views_map.contains_key(&name) {
            errors.push(ValidationError::DuplicateFunctionName { name });
        } else {
            views_map.insert(name, def);
        }
    }

    ErrorStream::add_extra_errors(Ok((reducers_map, procedures_map, views_map)), errors)
}

fn check_non_procedure_misc_exports(
    misc_exports: Vec<RawMiscModuleExportV9>,
    validator: &ModuleValidator,
    tables: &mut IdentifierMap<TableDef>,
) -> Result<()> {
    misc_exports
        .into_iter()
        .map(|export| match export {
            RawMiscModuleExportV9::ColumnDefaultValue(cdv) => process_column_default_value(&cdv, validator, tables),
            RawMiscModuleExportV9::Procedure(_proc) => {
                unreachable!("Procedure defs should already have been sorted out of `misc_exports`")
            }
            _ => unimplemented!("unknown misc export"),
        })
        .collect_all_errors::<()>()
}

fn process_column_default_value(
    cdv: &RawColumnDefaultValueV9,
    validator: &ModuleValidator,
    tables: &mut IdentifierMap<TableDef>,
) -> Result<()> {
    // Validate the default value
    let validated_value = validator.validate_column_default_value(tables, cdv)?;

    let table_name = identifier(cdv.table.clone())?;
    let table = tables
        .get_mut(&table_name)
        .ok_or_else(|| ValidationError::TableNotFound {
            table: cdv.table.clone(),
        })?;

    let column = table
        .columns
        .get_mut(cdv.col_id.idx())
        .ok_or_else(|| ValidationError::ColumnNotFound {
            table: cdv.table.clone(),
            def: cdv.table.clone(),
            column: cdv.col_id,
        })?;

    // Ensure there's only one default value.
    if column.default_value.is_some() {
        return Err(ValidationError::MultipleColumnDefaultValues {
            table: cdv.table.clone(),
            col_id: cdv.col_id,
        }
        .into());
    }

    // Set the default value
    column.default_value = Some(validated_value);

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::def::validate::tests::{
        check_product_type, expect_identifier, expect_raw_type_name, expect_resolve, expect_type_name,
    };
    use crate::def::{validate::Result, ModuleDef};
    use crate::def::{
        BTreeAlgorithm, ConstraintData, ConstraintDef, DirectAlgorithm, FunctionKind, IndexDef, SequenceDef,
        UniqueConstraintData,
    };
    use crate::error::*;
    use crate::type_for_generate::ClientCodegenError;

    use itertools::Itertools;
    use spacetimedb_data_structures::expect_error_matching;
    use spacetimedb_lib::db::raw_def::v9::{btree, direct};
    use spacetimedb_lib::db::raw_def::*;
    use spacetimedb_lib::ScheduleAt;
    use spacetimedb_primitives::{ColId, ColList, ColSet};
    use spacetimedb_sats::{AlgebraicType, AlgebraicTypeRef, AlgebraicValue, ProductType, SumValue};
    use v9::{Lifecycle, RawIndexAlgorithm, RawModuleDefV9Builder, TableAccess, TableType};

    /// This test attempts to exercise every successful path in the validation code.
    #[test]
    fn valid_definition() {
        let mut builder = RawModuleDefV9Builder::new();

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
            .with_schedule("check_deliveries", 1)
            .with_type(TableType::System)
            .finish();

        builder.add_reducer("init", ProductType::unit(), Some(Lifecycle::Init));
        builder.add_reducer("on_connect", ProductType::unit(), Some(Lifecycle::OnConnect));
        builder.add_reducer("on_disconnect", ProductType::unit(), Some(Lifecycle::OnDisconnect));
        builder.add_reducer("extra_reducer", ProductType::from([("a", AlgebraicType::U64)]), None);
        builder.add_reducer(
            "check_deliveries",
            ProductType::from([("a", deliveries_product_type.into())]),
            None,
        );

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
        let mut builder = RawModuleDefV9Builder::new();

        // `build_table` does NOT initialize table.product_type_ref, which should result in an error.
        builder.build_table("Bananas", AlgebraicTypeRef(1337)).finish();

        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::InvalidProductTypeRef { table, ref_ } => {
            &table[..] == "Bananas" && ref_ == &AlgebraicTypeRef(1337)
        });
    }

    #[test]
    fn not_canonically_ordered_columns() {
        let mut builder = RawModuleDefV9Builder::new();
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
        let mut builder = RawModuleDefV9Builder::new();
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
        let mut builder = RawModuleDefV9Builder::new();
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
        let mut builder = RawModuleDefV9Builder::new();
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
        let mut builder = RawModuleDefV9Builder::new();
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
        let mut builder = RawModuleDefV9Builder::new();
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
        let mut builder = RawModuleDefV9Builder::new();
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
        let mut builder = RawModuleDefV9Builder::new();
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
        let mut builder = RawModuleDefV9Builder::new();
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

        let mut builder = RawModuleDefV9Builder::new();
        let ref_ = builder.add_algebraic_type([], "Recursive", recursive_type.clone(), false);
        builder.add_reducer("silly", ProductType::from([("a", ref_.into())]), None);
        let result: ModuleDef = builder.finish().try_into().unwrap();

        assert!(result.typespace_for_generate[ref_].is_recursive());
    }

    #[test]
    fn out_of_bounds_ref() {
        let invalid_type_1 = AlgebraicType::product([("a", AlgebraicTypeRef(31).into())]);
        let mut builder = RawModuleDefV9Builder::new();
        let ref_ = builder.add_algebraic_type([], "Invalid", invalid_type_1.clone(), false);
        builder.add_reducer("silly", ProductType::from([("a", ref_.into())]), None);
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ClientCodegenError { location, error: ClientCodegenError::TypeRefError(_)  } => {
            location == &TypeLocation::InTypespace { ref_: AlgebraicTypeRef(0) }
        });
    }

    #[test]
    fn not_valid_for_client_code_generation() {
        let inner_type_invalid_for_use = AlgebraicType::product([("b", AlgebraicType::U32)]);
        let invalid_type = AlgebraicType::product([("a", inner_type_invalid_for_use.clone())]);
        let mut builder = RawModuleDefV9Builder::new();
        let ref_ = builder.add_algebraic_type([], "Invalid", invalid_type.clone(), false);
        builder.add_reducer("silly", ProductType::from([("a", ref_.into())]), None);
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
    fn hash_index_unsupported() {
        let mut builder = RawModuleDefV9Builder::new();
        builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                false,
            )
            .with_index(RawIndexAlgorithm::Hash { columns: 0.into() }, "bananas_b")
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::HashIndexUnsupported { index } => {
            &index[..] == "Bananas_b_idx_hash"
        });
    }

    #[test]
    fn unique_constrain_without_index() {
        let mut builder = RawModuleDefV9Builder::new();
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
        let mut builder = RawModuleDefV9Builder::new();
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
        let mut builder = RawModuleDefV9Builder::new();
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
        let mut builder = RawModuleDefV9Builder::new();
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
        let mut builder = RawModuleDefV9Builder::new();
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
        let mut builder = RawModuleDefV9Builder::new();
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
        let mut builder = RawModuleDefV9Builder::new();
        builder.add_reducer("init1", ProductType::unit(), Some(Lifecycle::Init));
        builder.add_reducer("init1", ProductType::unit(), Some(Lifecycle::Init));
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateLifecycle { lifecycle } => {
            lifecycle == &Lifecycle::Init
        });
    }

    #[test]
    fn missing_scheduled_reducer() {
        let mut builder = RawModuleDefV9Builder::new();
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
            .with_schedule("check_deliveries", 1)
            .with_type(TableType::System)
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::MissingScheduledFunction { schedule, function } => {
            &schedule[..] == "Deliveries_sched" &&
                function == &expect_identifier("check_deliveries")
        });
    }

    #[test]
    fn incorrect_scheduled_reducer_args() {
        let mut builder = RawModuleDefV9Builder::new();
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
            .with_schedule("check_deliveries", 1)
            .with_type(TableType::System)
            .finish();
        builder.add_reducer("check_deliveries", ProductType::from([("a", AlgebraicType::U64)]), None);
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
        let mut builder = RawModuleDefV9Builder::new();

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
            .with_schedule("check_deliveries", 1)
            .with_type(TableType::System)
            .finish();

        builder.add_reducer(
            "check_deliveries",
            ProductType::from([("a", deliveries_product_type.into())]),
            None,
        );

        // Our builder methods ignore the possibility of setting names at the moment.
        // But, it could be done in the future for some reason.
        // Check if it works.
        let mut raw_def = builder.finish();
        raw_def.tables[0].constraints[0].name = Some("wacky.constraint()".into());
        raw_def.tables[0].indexes[0].name = Some("wacky.index()".into());
        raw_def.tables[0].sequences[0].name = Some("wacky.sequence()".into());

        let def: ModuleDef = raw_def.try_into().unwrap();
        assert!(def.lookup::<ConstraintDef>("wacky.constraint()").is_some());
        assert!(def.lookup::<IndexDef>("wacky.index()").is_some());
        assert!(def.lookup::<SequenceDef>("wacky.sequence()").is_some());
    }

    #[test]
    fn duplicate_reducer_names() {
        let mut builder = RawModuleDefV9Builder::new();

        builder.add_reducer("foo", [("i", AlgebraicType::I32)].into(), None);
        builder.add_reducer("foo", [("name", AlgebraicType::String)].into(), None);

        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateFunctionName { name } => {
            &name[..] == "foo"
        });
    }

    #[test]
    fn duplicate_procedure_names() {
        let mut builder = RawModuleDefV9Builder::new();

        builder.add_procedure("foo", [("i", AlgebraicType::I32)].into(), AlgebraicType::unit());
        builder.add_procedure("foo", [("name", AlgebraicType::String)].into(), AlgebraicType::unit());

        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateFunctionName { name } => {
            &name[..] == "foo"
        });
    }

    #[test]
    fn duplicate_procedure_and_reducer_name() {
        let mut builder = RawModuleDefV9Builder::new();

        builder.add_reducer("foo", [("i", AlgebraicType::I32)].into(), None);
        builder.add_procedure("foo", [("i", AlgebraicType::I32)].into(), AlgebraicType::unit());

        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateFunctionName { name } => {
            &name[..] == "foo"
        });
    }
}
