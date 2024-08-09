use crate::def::*;
use crate::error::{RawColumnName, ValidationError};
use crate::{def::validate::Result, error::TypeLocation};
use spacetimedb_data_structures::error_stream::{CollectAllErrors, CombineErrors};
use spacetimedb_lib::db::{
    default_element_ordering::{product_type_has_default_ordering, sum_type_has_default_ordering},
    raw_def::v9::*,
};
use spacetimedb_lib::ProductType;
use spacetimedb_sats::WithTypespace;

/// Validate a `RawModuleDefV9` and convert it into a `ModuleDef`,
/// or return a stream of errors if the definition is invalid.
pub fn validate(def: RawModuleDefV9) -> Result<ModuleDef> {
    let RawModuleDefV9 {
        typespace,
        tables,
        reducers,
        types,
        misc_exports,
    } = def;

    let mut validator = Validator {
        typespace: &typespace,
        stored_in_table_def: HashMap::new(),
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

    let tables = tables
        .into_iter()
        .map(|table| {
            validator
                .validate_table_def(table)
                .map(|table_def| (table_def.name.clone(), table_def))
        })
        .collect_all_errors();

    let reducers = reducers
        .into_iter()
        .map(|reducer| {
            validator
                .validate_reducer_def(reducer)
                .map(|reducer_def| (reducer_def.name.clone(), reducer_def))
        })
        .collect_all_errors();

    let types = types
        .into_iter()
        .map(|ty| {
            validator
                .validate_type_def(ty)
                .map(|type_def| (type_def.name.clone(), type_def))
        })
        .collect_all_errors::<HashMap<_, _>>()
        .and_then(|types| {
            // We need to validate the typespace after we have all the type definitions.
            // This is because we need to check that every (non-nominal) type in the typespace
            // has a corresponding type definition.
            validator.validate_typespace(&types)?;
            Ok(types)
        });

    // It's statically impossible for this assert to fire until `RawMiscModuleExportV9` grows some variants.
    assert_eq!(
        misc_exports.len(),
        0,
        "Misc module exports are not yet supported in ABI v9."
    );

    let Validator {
        stored_in_table_def, ..
    } = validator;

    let (tables, reducers, types) = (tables, reducers, types)
        .combine_errors()
        .map_err(|errors| errors.sort_deduplicate())?;

    Ok(ModuleDef {
        tables,
        reducers,
        types,
        typespace,
        stored_in_table_def,
    })
}

/// Collects state used during validation.
struct Validator<'a> {
    /// The typespace of the module.
    ///
    /// Behind a reference to ensure we don't accidentally mutate it.
    typespace: &'a Typespace,

    /// Names we have seen so far.
    ///
    /// It would be nice if we could have span information here, but currently it isn't passed
    /// through the ABI boundary.
    /// We could add it as a `MiscModuleExport` later without breaking the ABI.
    stored_in_table_def: HashMap<Identifier, Identifier>,
}

impl Validator<'_> {
    fn validate_table_def(&mut self, table: RawTableDefV9) -> Result<TableDef> {
        let RawTableDefV9 {
            name: raw_table_name,
            product_type_ref,
            indexes,
            unique_constraints,
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

        let table_in_progress = TableInProgress {
            raw_name: &raw_table_name[..],
            product_type,
        };

        let columns = product_type
            .col_ids()
            .map(|id| self.validate_column_def(&table_in_progress, id))
            .collect_all_errors();

        let indexes = indexes
            .into_iter()
            .map(|index| {
                self.validate_index_def(&table_in_progress, index)
                    .map(|index| (index.name.clone(), index))
            })
            .collect_all_errors();

        let unique_constraints = unique_constraints
            .into_iter()
            .map(|constraint| {
                self.validate_unique_constraint_def(&table_in_progress, constraint)
                    .map(|constraint| (constraint.name.clone(), constraint))
            })
            .collect_all_errors();

        let sequences = sequences
            .into_iter()
            .map(|sequence| {
                self.validate_sequence_def(&table_in_progress, sequence)
                    .map(|sequence| (sequence.name.clone(), sequence))
            })
            .collect_all_errors();

        let schedule = schedule
            .map(|schedule| self.validate_schedule_def(&table_in_progress, schedule))
            .transpose();

        let name = identifier(raw_table_name);

        let (name, columns, indexes, unique_constraints, sequences, schedule) =
            (name, columns, indexes, unique_constraints, sequences, schedule).combine_errors()?;

        Ok(TableDef {
            name,
            product_type_ref,
            columns,
            indexes,
            unique_constraints,
            sequences,
            schedule,
            table_type,
            table_access,
        })
    }

    /// Validate a column.
    ///
    /// Note that this accepts a `ProductTypeElement` rather than a `ColumnDef`,
    /// because all information about columns is stored in the `Typespace` in ABI version 9.
    fn validate_column_def(&mut self, table_in_progress: &TableInProgress, col_id: ColId) -> Result<ColumnDef> {
        let column = &table_in_progress
            .product_type
            .get_column(col_id)
            .expect("enumerate is generating an out-of-range index...");

        let name: Result<Identifier> = column
            .name()
            .ok_or_else(|| {
                ValidationError::UnnamedColumn {
                    column: table_in_progress.raw_column_name(col_id),
                }
                .into()
            })
            .and_then(|name| identifier(name.into()));

        // We don't need to validate this type here.
        // This type comes from the table's `product_type_ref` field, which points into the `typespace`,
        // and validation for types in the typespace is handled elsewhere.
        let ty = column.algebraic_type.clone();

        // This error will be created multiple times if the table name is invalid,
        // but we sort and deduplicate the error stream afterwards,
        // so it isn't a huge deal.
        //
        // This is necessary because we require `ErrorStream` to be
        // nonempty. We need to put something in there if the table name is invalid.
        let table_name = identifier(table_in_progress.raw_name.into());

        let (name, table_name) = (name, table_name).combine_errors()?;

        Ok(ColumnDef {
            name,
            ty,
            col_id,
            table_name,
        })
    }

    fn validate_sequence_def(
        &mut self,
        table_in_progress: &TableInProgress,
        sequence: RawSequenceDefV9,
    ) -> Result<SequenceDef> {
        let RawSequenceDefV9 {
            name,
            column,
            min_value,
            start,
            max_value,
        } = sequence;

        // The column for the sequence exists and is an appropriate type.
        let column = table_in_progress.validate_col_id(&name, column).and_then(|col_id| {
            let ty = table_in_progress
                .product_type
                .get_column(col_id)
                .unwrap()
                .algebraic_type
                .clone();

            if !ty.is_integer() {
                Err(ValidationError::InvalidSequenceColumnType {
                    sequence: name.clone(),
                    column: table_in_progress.raw_column_name(col_id),
                    column_type: ty.clone(),
                }
                .into())
            } else {
                Ok(col_id)
            }
        });

        /// Compare two `Option<i128>` values, returning `true` if `lo <= hi`,
        /// or if either is undefined.
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

        let name = self.add_to_global_namespace(table_in_progress, name);

        let (name, column, (min_value, start, max_value)) = (name, column, min_start_max).combine_errors()?;

        Ok(SequenceDef {
            name,
            column,
            min_value,
            start,
            max_value,
        })
    }

    /// Validate an index definition.
    fn validate_index_def(&mut self, table_in_progress: &TableInProgress, index: RawIndexDefV9) -> Result<IndexDef> {
        let RawIndexDefV9 { name, algorithm } = index;

        let algorithm: Result<IndexAlgorithm> = match algorithm {
            RawIndexAlgorithm::BTree { columns } => table_in_progress
                .validate_col_ids(&name, columns)
                .map(|columns| IndexAlgorithm::BTree { columns }),
            _ => Err(ValidationError::OnlyBtree { index: name.clone() }.into()),
        };
        let name = identifier(name);

        let (name, algorithm) = (name, algorithm).combine_errors()?;

        let generated = false;

        Ok(IndexDef {
            name,
            algorithm,
            generated,
        })
    }

    /// Validate a unique constraint definition.
    fn validate_unique_constraint_def(
        &mut self,
        table_in_progress: &TableInProgress,
        constraint: RawUniqueConstraintDefV9,
    ) -> Result<UniqueConstraintDef> {
        let RawUniqueConstraintDefV9 { name, columns } = constraint;

        let columns = table_in_progress.validate_col_ids(&name, columns);
        let name = self.add_to_global_namespace(table_in_progress, name);

        let (name, columns) = (name, columns).combine_errors()?;
        Ok(UniqueConstraintDef { name, columns })
    }

    /// Validate a schedule definition.
    fn validate_schedule_def(
        &mut self,
        table_in_progress: &TableInProgress,
        schedule: RawScheduleDefV9,
    ) -> Result<ScheduleDef> {
        let RawScheduleDefV9 { name, reducer_name } = schedule;

        // Find the appropriate columns.
        let at_column = table_in_progress
            .product_type
            .elements
            .iter()
            .enumerate()
            .find(|(_, element)| element.name() == Some("scheduled_at"));
        let id_column = table_in_progress
            .product_type
            .elements
            .iter()
            .enumerate()
            .find(|(_, element)| {
                element.name() == Some("scheduled_id") && element.algebraic_type == AlgebraicType::U64
            });

        // Error if either column is missing.
        let at_id = at_column.zip(id_column).ok_or_else(|| {
            ValidationError::ScheduledIncorrectColumns {
                table: table_in_progress.raw_name.into(),
                columns: table_in_progress.product_type.clone(),
            }
            .into()
        });

        let name = identifier(name);
        let reducer_name = identifier(reducer_name);
        let (name, (at_column, id_column), reducer_name) = (name, at_id, reducer_name).combine_errors()?;
        let at_column = at_column.0.into();
        let id_column = id_column.0.into();

        Ok(ScheduleDef {
            name,
            at_column,
            id_column,
            reducer_name,
        })
    }

    /// Validate a reducer definition.
    fn validate_reducer_def(&mut self, reducer_der: RawReducerDefV9) -> Result<ReducerDef> {
        let RawReducerDefV9 { name, params } = reducer_der;

        let params_valid: Result<()> = params
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
                self.validate_nominal_algebraic_type(&location, &param.algebraic_type)
            })
            .collect_all_errors();

        let name = identifier(name);

        let (name, ()) = (name, params_valid).combine_errors()?;

        Ok(ReducerDef { name, params })
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
                if !custom_ordering {
                    let correct = match pointed_to {
                        AlgebraicType::Sum(sum) => sum_type_has_default_ordering(sum),
                        AlgebraicType::Product(product) => product_type_has_default_ordering(product),
                        _ => true,
                    };
                    if !correct {
                        return Err(ValidationError::TypeHasIncorrectOrdering {
                            type_name: name.clone(),
                            ref_: ty,
                            bad_type: pointed_to.clone(),
                        }
                        .into());
                    }
                }

                // note: we return the reference `ty`, not the pointed-to type `pointed_to`.
                // The reference is semantically important.
                Ok((ty, custom_ordering))
            });

        let name = identifier(name);

        let (name, (ty, custom_ordering)) = (name, ty_custom_ordering).combine_errors()?;

        Ok(TypeDef {
            name,
            ty,
            custom_ordering,
        })
    }

    /// Validate `name` as an `Identifier` and add it to the global namespace, registering the corresponding `Def` as being stored in a  particular `TableDef`.
    ///
    /// If it has already been added, return an error.
    ///
    /// This is not used for all `Def` types.
    fn add_to_global_namespace(&mut self, table_in_progress: &TableInProgress, name: Box<str>) -> Result<Identifier> {
        let table_name = identifier(table_in_progress.raw_name.into());
        let name = identifier(name);

        // This may report the table_name as invalid multiple times, but this will be removed
        // when we sort and deduplicate the error stream.
        let (table_name, name) = (table_name, name).combine_errors()?;
        if self.stored_in_table_def.contains_key(&name) {
            Err(ValidationError::DuplicateName { name }.into())
        } else {
            self.stored_in_table_def.insert(name.clone(), table_name);
            Ok(name)
        }
    }

    /// Validates that a type:
    /// - Is in nominal normal form,
    /// - Is not recursive,
    /// - Contains only valid refs.
    ///
    /// This is applied to:
    /// - fields of product and sum types in the typespace.
    /// - types in the typespace that aren't sums or products.
    /// - reducer arguments.
    ///
    /// Note that `TypeLocation` is defined using `Cow`, so calling this method
    /// does not need to allocate unless an error is returned.
    fn validate_nominal_algebraic_type(&mut self, location: &TypeLocation, ty: &AlgebraicType) -> Result<()> {
        let nominal: Result<()> = if ty.is_nominal_normal_form() {
            Ok(())
        } else {
            Err(ValidationError::NotNominalNormalForm {
                location: location.clone().make_static(),
                ty: ty.clone(),
            }
            .into())
        };
        // This repeats some work for nested types.
        // TODO: implement a reentrant, cached version of `resolve_refs`.
        let resolves_fine: Result<()> = WithTypespace::new(self.typespace, ty)
            .resolve_refs()
            .map(|_resolved| ())
            .map_err(|error| {
                ValidationError::ResolutionFailure {
                    location: location.clone().make_static(),
                    ty: ty.clone(),
                    error,
                }
                .into()
            });

        let ((), ()) = (nominal, resolves_fine).combine_errors()?;
        Ok(())
    }

    /// Validate the typespace.
    /// This checks that every `Product`, `Sum`, and `Ref` in the typespace has a corresponding
    /// `TypeDef`.
    fn validate_typespace(&mut self, validated_type_defs: &HashMap<Identifier, TypeDef>) -> Result<()> {
        let id_to_name = validated_type_defs
            .values()
            .map(|def| (&def.ty, &def.name))
            .collect::<HashMap<_, _>>();

        self.typespace
            .types
            .iter()
            .enumerate()
            .map(|(pos, ty)| {
                let ref_ = AlgebraicTypeRef(pos as u32);
                let location = TypeLocation::InTypespace { ref_ };

                // Check that the type is valid.
                let is_valid: Result<()> = match ty {
                    // If the type is in nominal normal form, we can validate it directly.
                    ty if ty.is_nominal_normal_form() => self.validate_nominal_algebraic_type(&location, ty),
                    // Otherwise, if the type is a sum or product, we need to validate its fields.
                    AlgebraicType::Sum(sum) => sum
                        .variants
                        .iter()
                        .map(|variant| self.validate_nominal_algebraic_type(&location, &variant.algebraic_type))
                        .collect_all_errors(),
                    AlgebraicType::Product(product) => product
                        .elements
                        .iter()
                        .map(|element| self.validate_nominal_algebraic_type(&location, &element.algebraic_type))
                        .collect_all_errors(),
                    _ => unreachable!("if type is not sum or product, it must be in nominal normal form"),
                };

                // Check that the type has a corresponding `TypeDef`.
                let has_def: Result<()> = if ty.is_nominal_normal_form() {
                    // Nominal types like the unit type, `Option<T>`, `Ref`s, etc may
                    // be stored in the typespace without a `TypeDef`.
                    Ok(())
                } else {
                    // If the type is a non-nominal sum or product, it requires a `TypeDef`.
                    match ty {
                        AlgebraicType::Sum(..) | AlgebraicType::Product(..) => {
                            let ref_ = AlgebraicTypeRef(pos as _);
                            if !id_to_name.contains_key(&ref_) {
                                Err(ValidationError::MissingTypeDef { ref_ }.into())
                            } else {
                                Ok(())
                            }
                        }
                        _ => Ok(()),
                    }
                };
                let ((), ()) = (is_valid, has_def).combine_errors()?;
                Ok(())
            })
            .collect_all_errors()
    }
}

/// A partially validated table.
struct TableInProgress<'a> {
    raw_name: &'a str,
    product_type: &'a ProductType,
}

impl TableInProgress<'_> {
    /// Validate a `ColId` for this table, returning it unmodified if valid.
    /// `def_name` is the name of the definition being validated and is used in errors.
    pub fn validate_col_id(&self, def_name: &str, col_id: ColId) -> Result<ColId> {
        if self.product_type.elements.get(col_id.0 as usize).is_some() {
            Ok(col_id)
        } else {
            Err(ValidationError::ColumnNotFound {
                column: col_id,
                table: self.raw_name.into(),
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
            .get_column(col_id)
            .and_then(|col| col.name())
            .map(|name| name.into())
            .unwrap_or_else(|| format!("{}", col_id).into());

        RawColumnName {
            table: self.raw_name.into(),
            column,
        }
    }
}

/// Helper to create an `Identifier` from a `str` with the appropriate error type.
/// TODO: memoize this.
fn identifier(name: Box<str>) -> Result<Identifier> {
    Identifier::new(name).map_err(|error| ValidationError::IdentifierError { error }.into())
}

#[cfg(test)]
mod tests {
    use crate::def::validate::tests::{expect_col_list, expect_identifier};
    use crate::def::{validate::Result, ModuleDef, TableDef};
    use crate::error::*;
    use crate::expect_error_matching;

    use spacetimedb_lib::db::raw_def::*;
    use spacetimedb_lib::ScheduleAt;
    use spacetimedb_sats::typespace::TypeRefError;
    use spacetimedb_sats::{AlgebraicType, AlgebraicTypeRef, ProductType, SumType, SumTypeVariant};
    use v9::{RawIndexAlgorithm, RawModuleDefV9Builder};

    /// Check that the columns of a `TableDef` correctly correspond the the `TableDef`'s
    /// `product_type_ref`.
    fn check_product_type(module_def: &ModuleDef, table_def: &TableDef) {
        let product_type = module_def
            .typespace
            .get(table_def.product_type_ref)
            .unwrap()
            .as_product()
            .unwrap();

        for (element, column) in product_type.elements.iter().zip(table_def.columns.iter()) {
            assert_eq!(element.name(), Some(&*column.name));
            assert_eq!(element.algebraic_type, column.ty);
        }
    }

    /// This test attempts to exercise every successful path in the validation code.
    #[test]
    fn valid_definition() {
        let mut builder = RawModuleDefV9Builder::new();

        let product_type = AlgebraicType::product([("a", AlgebraicType::U64), ("b", AlgebraicType::String)]);
        let product_type_ref = builder.add_type_for_tests("ReferencedProduct", product_type.clone(), false);

        let sum_type = AlgebraicType::Sum(SumType {
            variants: vec![
                SumTypeVariant::unit("Gala"),
                SumTypeVariant::unit("GrannySmith"),
                SumTypeVariant::unit("RedDelicious"),
            ]
            .into_boxed_slice(),
        });
        let sum_type_ref = builder.add_type_for_tests("ReferencedSum", sum_type.clone(), false);

        let schedule_at_type = builder.add_type::<ScheduleAt>();

        builder
            .build_table_for_tests(
                "Apples",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("name", AlgebraicType::String),
                    ("count", AlgebraicType::U16),
                    ("type", sum_type_ref.into()),
                ]),
                true,
            )
            .finish();

        builder
            .build_table_for_tests(
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
            .with_column_sequence(0, None)
            .with_unique_constraint(expect_col_list([0]), None)
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: expect_col_list([0]),
                },
                None,
            )
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: expect_col_list([0, 1, 2]),
                },
                None,
            )
            .finish();

        builder
            .build_table_for_tests(
                "Deliveries",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("scheduled_at", schedule_at_type.clone()),
                    ("scheduled_id", AlgebraicType::U64),
                ]),
                true,
            )
            .with_schedule("check_deliveries", Some("check_deliveries_schedule".into()))
            .finish();

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
        assert_eq!(apples_def.columns[3].ty, AlgebraicType::Ref(sum_type_ref));

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
            &delivery_def.schedule.as_ref().unwrap().reducer_name[..],
            "check_deliveries"
        );

        assert_eq!(def.typespace.get(product_type_ref), Some(&product_type));
        assert_eq!(def.typespace.get(sum_type_ref), Some(&sum_type));

        check_product_type(&def, &apples_def);
        check_product_type(&def, &bananas_def);
        check_product_type(&def, &delivery_def);
    }

    #[test]
    fn invalid_product_type_ref() {
        let mut builder = RawModuleDefV9Builder::new();

        // `with_table` does NOT initialize table.product_type_ref, which should result in an error.
        builder.build_table("Bananas".into(), AlgebraicTypeRef(1337)).finish();

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
            .build_table_for_tests("Bananas", product_type.clone(), false)
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::TypeHasIncorrectOrdering { type_name, ref_, bad_type } => {
            &type_name[..] == "Bananas" &&
            ref_ == &AlgebraicTypeRef(0) &&
            bad_type == &product_type.clone().into()
        });
    }

    #[test]
    fn invalid_table_name() {
        let mut builder = RawModuleDefV9Builder::new();
        builder
            .build_table_for_tests(
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
            .build_table_for_tests(
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
            .build_table_for_tests(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                false,
            )
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: expect_col_list([0, 55]),
                },
                Some("Bananas_index".into()),
            )
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ColumnNotFound { table, def, column } => {
            &table[..] == "Bananas" &&
            &def[..] == "Bananas_index" &&
            column == &55.into()
        });
    }

    #[test]
    fn invalid_unique_constraint_column_ref() {
        let mut builder = RawModuleDefV9Builder::new();
        builder
            .build_table_for_tests(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                false,
            )
            .with_unique_constraint(expect_col_list([55]), Some("Bananas_unique_constraint".into()))
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ColumnNotFound { table, def, column } => {
            &table[..] == "Bananas" &&
            &def[..] == "Bananas_unique_constraint" &&
            column == &55.into()
        });
    }

    #[test]
    fn invalid_sequence_column_ref() {
        // invalid column id
        let mut builder = RawModuleDefV9Builder::new();
        builder
            .build_table_for_tests(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                false,
            )
            .with_column_sequence(55, Some("Bananas_sequence".into()))
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ColumnNotFound { table, def, column } => {
            &table[..] == "Bananas" &&
            &def[..] == "Bananas_sequence" &&
            column == &55.into()
        });

        // incorrect column type
        let mut builder = RawModuleDefV9Builder::new();
        builder
            .build_table_for_tests(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::String)]),
                false,
            )
            .with_column_sequence(1, Some("Bananas_sequence".into()))
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::InvalidSequenceColumnType { sequence, column, column_type } => {
            &sequence[..] == "Bananas_sequence" &&
            column == &RawColumnName::new("Bananas", "a") &&
            column_type == &AlgebraicType::String
        });
    }

    #[test]
    fn invalid_index_column_duplicates() {
        let mut builder = RawModuleDefV9Builder::new();
        builder
            .build_table_for_tests(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                false,
            )
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: expect_col_list([0, 0]),
                },
                Some("Bananas_index".into()),
            )
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateColumns{ def, columns } => {
            &def[..] == "Bananas_index" && columns == &expect_col_list([0, 0])
        });
    }

    #[test]
    fn invalid_unique_constraint_column_duplicates() {
        let mut builder = RawModuleDefV9Builder::new();
        builder
            .build_table_for_tests(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                false,
            )
            .with_unique_constraint(expect_col_list([1, 1]), Some("Bananas_unique_constraint".into()))
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::DuplicateColumns{ def, columns } => {
            &def[..] == "Bananas_unique_constraint" && columns == &expect_col_list([1, 1])
        });
    }

    #[test]
    fn recursive_type_ref() {
        let recursive_type = AlgebraicType::product([("a", AlgebraicTypeRef(0).into())]);

        let mut builder = RawModuleDefV9Builder::new();
        builder.add_type_for_tests("Recursive", recursive_type.clone(), false);
        builder.add_reducer("silly", ProductType::from([("a", recursive_type.clone())]));
        let result: Result<ModuleDef> = builder.finish().try_into();

        // If you use a recursive type as a reducer argument, you get two errors.
        // One for the reducer argument, and one for the type itself.
        // This seems fine...
        expect_error_matching!(result, ValidationError::ResolutionFailure { location, ty, error } => {
            location == &TypeLocation::InTypespace { ref_: AlgebraicTypeRef(0) } &&
            ty == &recursive_type &&
            error == &TypeRefError::RecursiveTypeRef(AlgebraicTypeRef(0))
        });
        expect_error_matching!(result, ValidationError::ResolutionFailure { location, ty, error } => {
            location == &TypeLocation::ReducerArg {
                reducer_name: "silly".into(),
                position: 0,
                arg_name: Some("a".into())
            } &&
            ty == &recursive_type &&
            error == &TypeRefError::RecursiveTypeRef(AlgebraicTypeRef(0))
        });
    }

    #[test]
    fn invalid_type_ref() {
        let invalid_type_1 = AlgebraicType::product([("a", AlgebraicTypeRef(31).into())]);
        let invalid_type_2 = AlgebraicType::option(AlgebraicTypeRef(55).into());
        let mut builder = RawModuleDefV9Builder::new();
        builder.add_type_for_tests("Invalid", invalid_type_1.clone(), false);
        builder.add_reducer("silly", ProductType::from([("a", invalid_type_2.clone())]));
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::ResolutionFailure { location, ty, error } => {
            location == &TypeLocation::InTypespace { ref_: AlgebraicTypeRef(0) } &&
            ty == &invalid_type_1 &&
            error == &TypeRefError::InvalidTypeRef(AlgebraicTypeRef(31))
        });
        expect_error_matching!(result, ValidationError::ResolutionFailure { location, ty, error } => {
            location == &TypeLocation::ReducerArg {
                reducer_name: "silly".into(),
                position: 0,
                arg_name: Some("a".into())
            } &&
            ty == &invalid_type_2 &&
            error == &TypeRefError::InvalidTypeRef(AlgebraicTypeRef(55))
        });
    }

    #[test]
    fn only_btree_indexes() {
        let mut builder = RawModuleDefV9Builder::new();
        builder
            .build_table_for_tests(
                "Bananas",
                ProductType::from([("b", AlgebraicType::U16), ("a", AlgebraicType::U64)]),
                false,
            )
            .with_index(
                RawIndexAlgorithm::Hash {
                    columns: expect_col_list([0]),
                },
                Some("Bananas_index".into()),
            )
            .finish();
        let result: Result<ModuleDef> = builder.finish().try_into();

        expect_error_matching!(result, ValidationError::OnlyBtree { index } => {
            &index[..] == "Bananas_index"
        });
    }
}
