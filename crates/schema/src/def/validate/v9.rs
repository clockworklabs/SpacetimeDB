use crate::def::*;
use crate::error::{RawColumnName, ValidationError};
use crate::type_for_generate::{ClientCodegenError, ProductTypeDef, TypespaceForGenerateBuilder};
use crate::{def::validate::Result, error::TypeLocation};
use spacetimedb_data_structures::error_stream::{CollectAllErrors, CombineErrors};
use spacetimedb_data_structures::map::HashSet;
use spacetimedb_lib::db::default_element_ordering::{product_type_has_default_ordering, sum_type_has_default_ordering};
use spacetimedb_lib::ProductType;

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
        .map(|reducer| {
            validator
                .validate_reducer_def(reducer)
                .map(|reducer_def| (reducer_def.name.clone(), reducer_def))
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

    // It's statically impossible for this assert to fire until `RawMiscModuleExportV9` grows some variants.
    assert_eq!(
        misc_exports.len(),
        0,
        "Misc module exports are not yet supported in ABI v9."
    );

    let tables_types_reducers = (tables, types, reducers)
        .combine_errors()
        .and_then(|(tables, types, reducers)| {
            check_scheduled_reducers_exist(&tables, &reducers)?;
            Ok((tables, types, reducers))
        });

    let ModuleValidator {
        stored_in_table_def,
        typespace_for_generate,
        ..
    } = validator;

    let (tables, types, reducers) = (tables_types_reducers).map_err(|errors| errors.sort_deduplicate())?;

    let typespace_for_generate = typespace_for_generate.finish();

    let mut result = ModuleDef {
        tables,
        reducers,
        types,
        typespace,
        typespace_for_generate,
        stored_in_table_def,
        refmap,
        row_level_security_raw,
    };

    result.generate_indexes();

    Ok(result)
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
    lifecycle_reducers: HashSet<Lifecycle>,
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
            .collect_all_errors();

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

        let (name, columns, indexes, (constraints, primary_key), sequences, schedule) =
            (name, columns, indexes, constraints_primary_key, sequences, schedule).combine_errors()?;

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

    /// Validate a reducer definition.
    fn validate_reducer_def(&mut self, reducer_def: RawReducerDefV9) -> Result<ReducerDef> {
        let RawReducerDefV9 {
            name,
            params,
            lifecycle,
        } = reducer_def;

        let params_for_generate: Result<_> = params
            .elements
            .iter()
            .enumerate()
            .map(|(position, param)| {
                // Note: this does not allocate, since `TypeLocation` is defined using `Cow`.
                // We only allocate if an error is returned.
                let location = TypeLocation::ReducerArg {
                    reducer_name: (&*name).into(),
                    position,
                    arg_name: param.name().map(Into::into),
                };
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
            .collect_all_errors();

        // reducers don't live in the global namespace.
        let name = identifier(name);

        let lifecycle = lifecycle
            .map(|lifecycle| match self.lifecycle_reducers.insert(lifecycle.clone()) {
                true => Ok(lifecycle),
                false => Err(ValidationError::DuplicateLifecycle { lifecycle }.into()),
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
            _ => Err(ValidationError::OnlyBtree { index: name.clone() }.into()),
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
            reducer_name,
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
        let reducer_name = identifier(reducer_name);

        let (name, (at_column, id_column), reducer_name) = (name, at_id, reducer_name).combine_errors()?;

        Ok(ScheduleDef {
            name,
            at_column,
            id_column,
            reducer_name,
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
            .unwrap_or_else(|| format!("{}", col_id).into());

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
    selected.iter().map(|col| column_name(table_type, col)).join(",")
}

/// All indexes have this name format.
pub fn generate_index_name(table_name: &str, table_type: &ProductType, algorithm: &RawIndexAlgorithm) -> RawIdentifier {
    let (label, columns) = match algorithm {
        RawIndexAlgorithm::BTree { columns } => ("btree", columns),
        RawIndexAlgorithm::Hash { columns } => ("hash", columns),
        _ => unimplemented!("Unknown index algorithm {:?}", algorithm),
    };
    let column_names = concat_column_names(table_type, columns);
    format!("index.{label}({table_name},[{column_names}])").into()
}

/// All sequences have this name format.
pub fn generate_sequence_name(table_name: &str, table_type: &ProductType, column: ColId) -> RawIdentifier {
    let column_name = column_name(table_type, column);
    format!("sequence({table_name},{column_name})").into()
}

/// All schedules have this name format.
pub fn generate_schedule_name(table_name: &str) -> RawIdentifier {
    format!("schedule({table_name})").into()
}

/// All unique constraints have this name format.
pub fn generate_unique_constraint_name(
    table_name: &str,
    product_type: &ProductType,
    columns: &ColList,
) -> RawIdentifier {
    let column_names = concat_column_names(product_type, columns);
    format!("constraint.unique({table_name},[{column_names}])").into()
}

/// Helper to create an `Identifier` from a `str` with the appropriate error type.
/// TODO: memoize this.
fn identifier(name: Box<str>) -> Result<Identifier> {
    Identifier::new(name).map_err(|error| ValidationError::IdentifierError { error }.into())
}

fn check_scheduled_reducers_exist(
    tables: &IdentifierMap<TableDef>,
    reducers: &IndexMap<Identifier, ReducerDef>,
) -> Result<()> {
    tables
        .values()
        .map(|table| -> Result<()> {
            if let Some(schedule) = &table.schedule {
                let reducer = reducers.get(&schedule.reducer_name);
                if let Some(reducer) = reducer {
                    if reducer.params.elements.len() == 1
                        && reducer.params.elements[0].algebraic_type == table.product_type_ref.into()
                    {
                        Ok(())
                    } else {
                        Err(ValidationError::IncorrectScheduledReducerParams {
                            reducer: (&*schedule.reducer_name).into(),
                            expected: AlgebraicType::product([AlgebraicType::Ref(table.product_type_ref)]).into(),
                            actual: reducer.params.clone().into(),
                        }
                        .into())
                    }
                } else {
                    Err(ValidationError::MissingScheduledReducer {
                        schedule: schedule.name.clone(),
                        reducer: schedule.reducer_name.clone(),
                    }
                    .into())
                }
            } else {
                Ok(())
            }
        })
        .collect_all_errors()
}

#[cfg(test)]
mod tests {
    use crate::def::validate::tests::{
        check_product_type, expect_identifier, expect_raw_type_name, expect_resolve, expect_type_name,
    };
    use crate::def::{validate::Result, ModuleDef};
    use crate::def::{BTreeAlgorithm, ConstraintData, ConstraintDef, IndexDef, SequenceDef, UniqueConstraintData};
    use crate::error::*;
    use crate::type_for_generate::ClientCodegenError;

    use spacetimedb_data_structures::expect_error_matching;
    use spacetimedb_lib::db::raw_def::*;
    use spacetimedb_lib::ScheduleAt;
    use spacetimedb_primitives::{col_list, ColId, ColList};
    use spacetimedb_sats::{AlgebraicType, AlgebraicTypeRef, ProductType};
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
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: ColList::from_iter([1, 2]),
                },
                "apples_id",
            )
            .with_unique_constraint(3)
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
            .with_index(RawIndexAlgorithm::BTree { columns: 0.into() }, "bananas_count")
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: ColList::from_iter([0, 1, 2]),
                },
                "bananas_count_id_name",
            )
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
        assert_eq!(apples_def.columns[1].name, expect_identifier("name"));
        assert_eq!(apples_def.columns[1].ty, AlgebraicType::String);
        assert_eq!(apples_def.columns[2].name, expect_identifier("count"));
        assert_eq!(apples_def.columns[2].ty, AlgebraicType::U16);
        assert_eq!(apples_def.columns[3].name, expect_identifier("type"));
        assert_eq!(apples_def.columns[3].ty, sum_type_ref.into());
        assert_eq!(expect_resolve(&def.typespace, &apples_def.columns[3].ty), sum_type);

        assert_eq!(apples_def.primary_key, None);

        assert_eq!(apples_def.constraints.len(), 1);
        let apples_unique_constraint = "constraint.unique(Apples,[type])";
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

        assert_eq!(apples_def.indexes.len(), 2);
        for index in apples_def.indexes.values() {
            match &index.name[..] {
                // manually added
                "index.btree(Apples,[name,count])" => {
                    assert_eq!(
                        index.algorithm,
                        BTreeAlgorithm {
                            columns: ColList::from_iter([1, 2])
                        }
                        .into()
                    );
                    assert_eq!(index.accessor_name, Some(expect_identifier("apples_id")));
                }
                // auto-generated for the unique constraint
                _ => {
                    assert_eq!(index.algorithm, BTreeAlgorithm { columns: 3.into() }.into());
                    assert_eq!(index.accessor_name, None);
                }
            }
        }

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
            &delivery_def.schedule.as_ref().unwrap().reducer_name[..],
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
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: ColList::from_iter([0, 55]),
                },
                "bananas_a_b",
            )
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ColumnNotFound { table, def, column } => {
            &table[..] == "Bananas" &&
            &def[..] == "index.btree(Bananas,[b,col_55])" &&
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
            &def[..] == "constraint.unique(Bananas,[col_55])" &&
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
            &def[..] == "sequence(Bananas,col_55)" &&
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
            &sequence[..] == "sequence(Bananas,a)" &&
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
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: ColList::from_iter([0, 0]),
                },
                "bananas_b_b",
            )
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateColumns{ def, columns } => {
            &def[..] == "index.btree(Bananas,[b,b])" && columns == &ColList::from_iter([0, 0])
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
            &def[..] == "constraint.unique(Bananas,[a,a])" && columns == &ColList::from_iter([1, 1])
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
    fn only_btree_indexes() {
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

        expect_error_matching!(result, ValidationError::OnlyBtree { index } => {
            &index[..] == "index.hash(Bananas,[b])"
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
            .with_schedule("check_deliveries", 1)
            .with_type(TableType::System)
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::MissingScheduledReducer { schedule, reducer } => {
            &schedule[..] == "schedule(Deliveries)" &&
            reducer == &expect_identifier("check_deliveries")
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
            .with_schedule("check_deliveries", 1)
            .with_type(TableType::System)
            .finish();
        builder.add_reducer("check_deliveries", ProductType::from([("a", AlgebraicType::U64)]), None);
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::IncorrectScheduledReducerParams { reducer, expected, actual } => {
            &reducer[..] == "check_deliveries" &&
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
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: col_list![0, 2],
                },
                "nice_index_name",
            )
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
}
