//! `AlgebraicType` extensions for generating client code.

use spacetimedb_data_structures::{
    error_stream::{CollectAllErrors, CombineErrors, ErrorStream},
    map::{HashMap, HashSet},
};
use spacetimedb_lib::{AlgebraicType, ProductTypeElement};
use spacetimedb_sats::{typespace::TypeRefError, AlgebraicTypeRef, ArrayType, SumTypeVariant, Typespace};
use std::sync::Arc;

use crate::{
    error::{IdentifierError, PrettyAlgebraicType},
    identifier::Identifier,
};

/// Errors that can occur when rearranging types for client codegen.
#[derive(thiserror::Error, Debug, PartialOrd, Ord, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClientCodegenError {
    #[error(
        "internal codegen error: non-special product or sum type {ty} cannot be used to generate a client type use"
    )]
    NonSpecialTypeNotAUse { ty: PrettyAlgebraicType },

    #[error("internal codegen error: invalid AlgebraicTypeRef")]
    TypeRefError(#[from] TypeRefError),

    #[error("internal codegen error: type ref {ref_} was not pre-declared as a definition")]
    NonDeclaredTypeDef { ref_: AlgebraicTypeRef },

    #[error("internal codegen error: all type elements require names: {ty}")]
    NamelessTypeDef { ty: PrettyAlgebraicType },

    #[error("internal codegen error: type {ty} is not valid for generating a definition")]
    NotValidForDefinition { ty: PrettyAlgebraicType },

    #[error("type {ty} contains identifier error {err}")]
    NotValidIdentifier {
        ty: PrettyAlgebraicType,
        err: IdentifierError,
    },
}

type Result<T> = std::result::Result<T, ErrorStream<ClientCodegenError>>;

/// A typespace for generating client code.
///
/// The key difference is that this typespace stores only `AlgebraicTypeDef`s, not `AlgebraicType`s.
/// We use the same `AlgebraicTypeRef`s from the original typespace.
/// The difference is that `AlgebraicTypeRef`s ONLY point to `AlgebraicTypeDef`s.
/// Chains of `AlgebraicTypeRef`s in the original `Typespace` are contracted to point to their ending `AlgebraicTypeDef`.
#[derive(Debug, Clone)]
pub struct TypespaceForGenerate {
    defs: HashMap<AlgebraicTypeRef, AlgebraicTypeDef>,
}

impl TypespaceForGenerate {
    /// Get the definitions of the typespace.
    pub fn defs(&self) -> &HashMap<AlgebraicTypeRef, AlgebraicTypeDef> {
        &self.defs
    }

    /// Build a `TypespaceForGenerate`.
    ///
    /// We're required to declare known definitions up front.
    /// This is required for distinguishing between a use of the unit type, and a reference to a type declaration of a product type with no elements.
    pub fn builder(
        typespace: &Typespace,
        known_definitions: impl IntoIterator<Item = AlgebraicTypeRef>,
    ) -> TypespaceForGenerateBuilder<'_> {
        TypespaceForGenerateBuilder {
            typespace,
            result: TypespaceForGenerate { defs: HashMap::new() },
            known_definitions: known_definitions.into_iter().collect(),
            uses: HashSet::new(),
            known_uses: HashMap::new(),
            currently_touching: HashSet::new(),
        }
    }
}

/// An algebraic type definition.
#[derive(Debug, Clone)]
pub enum AlgebraicTypeDef {
    /// A product type declaration.
    Product(ProductTypeDef),
    /// A sum type declaration.
    Sum(SumTypeDef),
}

/// A product type definition.
#[derive(Debug, Clone)]
pub struct ProductTypeDef {
    /// The elements of the product type, in order.
    pub elements: Box<[(Identifier, AlgebraicTypeUse)]>,
}

/// A sum type definition.
#[derive(Debug, Clone)]
pub struct SumTypeDef {
    /// The variants of the sum type, in order.
    pub variants: Box<[(Identifier, AlgebraicTypeUse)]>,
}

/// A use of an algebraic type.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum AlgebraicTypeUse {
    /// A type where the definition is given by the typing context (`Typespace`).
    /// In other words, this is defined by a pointer to another `AlgebraicType`.
    /// An AlgebraicTypeUse must point to an `AlgebraicTypeDef`.
    Ref(AlgebraicTypeRef),

    /// The type of array values where elements are of a base type `elem_ty`.
    /// Values [`AlgebraicValue::Array(array)`](crate::AlgebraicValue::Array) will have this type.
    /// Stores an `Arc` for compression, the `Arc` is not semantically meaningful.
    Array(Arc<AlgebraicTypeUse>),

    /// The type of map values consisting of a key type `key_ty` and value `ty`.
    /// Values [`AlgebraicValue::Map(map)`](crate::AlgebraicValue::Map) will have this type.
    /// The order of entries in a map value is observable.
    /// Stores `Arc`s for compression, the `Arc` is not semantically meaningful.
    Map {
        key: Arc<AlgebraicTypeUse>,
        value: Arc<AlgebraicTypeUse>,
    },

    /// A standard structural option type.
    Option(Arc<AlgebraicTypeUse>),

    /// The special `ScheduleAt` type.
    ScheduleAt,

    /// The special `Identity` type.
    Identity,

    /// The special `Address` type.
    Address,

    /// The unit type (empty product).
    /// This is *distinct* from a use of a definition of a product type with no elements.
    Unit,

    /// The never type (empty sum).
    /// This is *distinct* from a use of a definition of a product type with no elements.
    Never,

    /// The UTF-8 encoded `String` type.
    String,

    /// The bool type. Values [`AlgebraicValue::Bool(b)`](crate::AlgebraicValue::Bool) will have this type.
    Bool,
    /// The `I8` type. Values [`AlgebraicValue::I8(v)`](crate::AlgebraicValue::I8) will have this type.
    I8,
    /// The `U8` type. Values [`AlgebraicValue::U8(v)`](crate::AlgebraicValue::U8) will have this type.
    U8,
    /// The `I16` type. Values [`AlgebraicValue::I16(v)`](crate::AlgebraicValue::I16) will have this type.
    I16,
    /// The `U16` type. Values [`AlgebraicValue::U16(v)`](crate::AlgebraicValue::U16) will have this type.
    U16,
    /// The `I32` type. Values [`AlgebraicValue::I32(v)`](crate::AlgebraicValue::I32) will have this type.
    I32,
    /// The `U32` type. Values [`AlgebraicValue::U32(v)`](crate::AlgebraicValue::U32) will have this type.
    U32,
    /// The `I64` type. Values [`AlgebraicValue::I64(v)`](crate::AlgebraicValue::I64) will have this type.
    I64,
    /// The `U64` type. Values [`AlgebraicValue::U64(v)`](crate::AlgebraicValue::U64) will have this type.
    U64,
    /// The `I128` type. Values [`AlgebraicValue::I128(v)`](crate::AlgebraicValue::I128) will have this type.
    I128,
    /// The `U128` type. Values [`AlgebraicValue::U128(v)`](crate::AlgebraicValue::U128) will have this type.
    U128,
    /// The `I256` type. Values [`AlgebraicValue::I256(v)`](crate::AlgebraicValue::I256) will have this type.
    I256,
    /// The `U256` type. Values [`AlgebraicValue::U256(v)`](crate::AlgebraicValue::U256) will have this type.
    U256,
    /// The `F32` type. Values [`AlgebraicValue::F32(v)`](crate::AlgebraicValue::F32) will have this type.
    F32,
    /// The `F64` type. Values [`AlgebraicValue::F64(v)`](crate::AlgebraicValue::F64) will have this type.
    F64,
}

/// A builder for a `TypespaceForGenerate`.
///
/// This is complicated by the fact that a typespace can store both *uses* and *definitions* of types.
pub struct TypespaceForGenerateBuilder<'a> {
    /// The original typespace.
    typespace: &'a Typespace,
    /// The result we are building.
    result: TypespaceForGenerate,
    /// The AlgebraicTypeRefs that we know point to definitions. Must be declared at the start of building.
    /// This is necessary to disambiguate between a use of the unit type, and a reference to a type declaration of a product type with no elements.
    known_definitions: HashSet<AlgebraicTypeRef>,
    /// Interning data structure, no semantic meaning.
    uses: HashSet<Arc<AlgebraicTypeUse>>,
    /// AlgebraicTypeRefs that point to uses.
    known_uses: HashMap<AlgebraicTypeRef, AlgebraicTypeUse>,
    /// Used for cycle detection.
    currently_touching: HashSet<AlgebraicTypeRef>,
}

impl TypespaceForGenerateBuilder<'_> {
    /// Finish building the `TypespaceForGenerate`.
    pub fn finish(self) -> TypespaceForGenerate {
        self.result
    }

    /// Add a *type use* to the typespace.
    /// This is a *use* of a type, not a definition.
    /// The input must satisfy `AlgebraicType::is_valid_for_client_type_use`.
    /// The input must resolve successfully.
    /// The input must not contain any cyclic definitions.
    pub fn add_use(&mut self, ty: &AlgebraicType) -> Result<AlgebraicTypeUse> {
        if ty.is_address() {
            Ok(AlgebraicTypeUse::Address)
        } else if ty.is_identity() {
            Ok(AlgebraicTypeUse::Identity)
        } else if ty.is_unit() {
            Ok(AlgebraicTypeUse::Unit)
        } else if ty.is_never() {
            Ok(AlgebraicTypeUse::Never)
        } else if let Some(elem_ty) = ty.as_option() {
            let elem_ty = self.add_use(elem_ty)?;
            let interned = self.intern_use(elem_ty);
            Ok(AlgebraicTypeUse::Option(interned))
        } else if ty.is_schedule_at() {
            Ok(AlgebraicTypeUse::ScheduleAt)
        } else {
            match ty {
                AlgebraicType::Ref(ref_) => {
                    // This is the only seriously complicated case, which has to deal with cycle detection.
                    let ref_ = *ref_;

                    if self.known_definitions.contains(&ref_) {
                        self.add_definition(ref_)?;
                        // The ref points to a known definition, so it must be a use.
                        Ok(AlgebraicTypeUse::Ref(ref_))
                    } else if let Some(use_) = self.known_uses.get(&ref_) {
                        // The ref is to a use which we have already seen.
                        Ok(use_.clone())
                    } else {
                        // We haven't processed it yet. It's either a ref to a valid use, or invalid.
                        let def = self
                            .typespace
                            .get(ref_)
                            .ok_or(TypeRefError::InvalidTypeRef(ref_))
                            .and_then(|def| {
                                if def == &AlgebraicType::Ref(ref_) {
                                    // Self-reference.
                                    Err(TypeRefError::RecursiveTypeRef(ref_))
                                } else {
                                    Ok(def)
                                }
                            })
                            .map_err(|e| ErrorStream::from(ClientCodegenError::TypeRefError(e)))?;

                        self.add_to_touching(ref_)?;

                        // Recurse.
                        let use_ = self.add_use(def)?;

                        self.remove_from_touching(ref_);

                        self.known_uses.insert(ref_, use_.clone());

                        Ok(use_)
                    }
                }
                AlgebraicType::Array(ArrayType { elem_ty }) => {
                    let elem_ty = self.add_use(elem_ty)?;
                    let interned = self.intern_use(elem_ty);
                    Ok(AlgebraicTypeUse::Array(interned))
                }
                AlgebraicType::Map(map) => {
                    let key_ty = self.add_use(&map.key_ty);
                    let value_ty = self.add_use(&map.ty);
                    let (key_ty, value_ty) = (key_ty, value_ty).combine_errors()?;
                    let interned_key = self.intern_use(key_ty);
                    let interned_value = self.intern_use(value_ty);
                    Ok(AlgebraicTypeUse::Map {
                        key: interned_key,
                        value: interned_value,
                    })
                }

                AlgebraicType::String => Ok(AlgebraicTypeUse::String),
                AlgebraicType::Bool => Ok(AlgebraicTypeUse::Bool),
                AlgebraicType::I8 => Ok(AlgebraicTypeUse::I8),
                AlgebraicType::U8 => Ok(AlgebraicTypeUse::U8),
                AlgebraicType::I16 => Ok(AlgebraicTypeUse::I16),
                AlgebraicType::U16 => Ok(AlgebraicTypeUse::U16),
                AlgebraicType::I32 => Ok(AlgebraicTypeUse::I32),
                AlgebraicType::U32 => Ok(AlgebraicTypeUse::U32),
                AlgebraicType::I64 => Ok(AlgebraicTypeUse::I64),
                AlgebraicType::U64 => Ok(AlgebraicTypeUse::U64),
                AlgebraicType::I128 => Ok(AlgebraicTypeUse::I128),
                AlgebraicType::U128 => Ok(AlgebraicTypeUse::U128),
                AlgebraicType::I256 => Ok(AlgebraicTypeUse::I256),
                AlgebraicType::U256 => Ok(AlgebraicTypeUse::U256),
                AlgebraicType::F32 => Ok(AlgebraicTypeUse::F32),
                AlgebraicType::F64 => Ok(AlgebraicTypeUse::F64),
                ty @ (AlgebraicType::Product(_) | AlgebraicType::Sum(_)) => {
                    Err(ErrorStream::from(ClientCodegenError::NonSpecialTypeNotAUse {
                        ty: PrettyAlgebraicType(ty.clone()),
                    }))
                }
            }
        }
    }

    /// Add a definition from the typespace.
    /// The definition must have been declared at the start of building.
    /// The input must satisfy `AlgebraicType::is_valid_for_client_type_definition` and not `AlgebraicType::is_valid_for_client_type_use`.
    pub fn add_definition(&mut self, ref_: AlgebraicTypeRef) -> Result<()> {
        if self.result.defs.contains_key(&ref_) {
            return Ok(());
        }
        if !self.known_definitions.contains(&ref_) {
            return Err(ClientCodegenError::NonDeclaredTypeDef { ref_ }.into());
        }

        let def = self
            .typespace
            .get(ref_)
            .ok_or_else(|| ErrorStream::from(ClientCodegenError::TypeRefError(TypeRefError::InvalidTypeRef(ref_))))?;

        self.add_to_touching(ref_)?;

        let result = match def {
            AlgebraicType::Product(product) => product
                .elements
                .iter()
                .map(|ProductTypeElement { name, algebraic_type }| self.process_element(def, name, algebraic_type))
                .collect_all_errors()
                .map(|elements| {
                    self.result
                        .defs
                        .insert(ref_, AlgebraicTypeDef::Product(ProductTypeDef { elements }));
                }),
            AlgebraicType::Sum(sum) => sum
                .variants
                .iter()
                .map(|SumTypeVariant { name, algebraic_type }| self.process_element(def, name, algebraic_type))
                .collect_all_errors()
                .map(|variants| {
                    self.result
                        .defs
                        .insert(ref_, AlgebraicTypeDef::Sum(SumTypeDef { variants }));
                }),
            _ => Err(ClientCodegenError::NotValidForDefinition {
                ty: PrettyAlgebraicType(def.clone()),
            }
            .into()),
        };
        self.remove_from_touching(ref_);
        result
    }

    /// Implements cycle detection.
    fn add_to_touching(&mut self, ref_: AlgebraicTypeRef) -> Result<()> {
        let already_present = !self.currently_touching.insert(ref_);
        if already_present {
            // We're already touching ref_! Error!

            // We don't empty out currently_touching so that a similar error will be returned
            // if we attempt to resolve this again.
            // ErrorStream deduplication by the caller will avoid saturating the user with errors.

            Err(ErrorStream::from(ClientCodegenError::TypeRefError(
                TypeRefError::RecursiveTypeRef(ref_),
            )))
        } else {
            Ok(())
        }
    }

    fn remove_from_touching(&mut self, ref_: AlgebraicTypeRef) {
        let removed = self.currently_touching.remove(&ref_);
        assert!(removed, "Internal invariant violated");
    }

    /// Process an element of a product or sum type.
    fn process_element(
        &mut self,
        def: &AlgebraicType,
        name: &Option<Box<str>>,
        algebraic_type: &AlgebraicType,
    ) -> Result<(Identifier, AlgebraicTypeUse)> {
        let name = name
            .as_ref()
            .ok_or_else(|| ErrorStream::from(ClientCodegenError::NamelessTypeDef { ty: def.clone().into() }))
            .and_then(|name| {
                Identifier::new(name.clone()).map_err(|err| {
                    ErrorStream::from(ClientCodegenError::NotValidIdentifier {
                        ty: def.clone().into(),
                        err,
                    })
                })
            });
        let ty = self.add_use(algebraic_type);
        (name, ty).combine_errors()
    }

    // Intern a use.
    // This is only used on types *inside* Map, Array, and Option types.
    fn intern_use(&mut self, use_: AlgebraicTypeUse) -> Arc<AlgebraicTypeUse> {
        if let Some(ty) = self.uses.get(&use_) {
            return ty.clone();
        }
        let ty = Arc::new(use_);
        self.uses.insert(ty.clone());
        ty
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use spacetimedb_data_structures::expect_error_matching;
    use spacetimedb_lib::AlgebraicType;
    use spacetimedb_sats::proptest::generate_typespace_valid_for_codegen;

    fn known_definitions(typespace: &Typespace) -> HashSet<AlgebraicTypeRef> {
        typespace
            .refs_with_types()
            .filter_map(|(ref_, ty)| {
                if ty.is_valid_for_client_type_definition() {
                    Some(ref_)
                } else {
                    None
                }
            })
            .collect()
    }

    proptest! {
        #[test]
        fn test_valid_typespace(t in generate_typespace_valid_for_codegen(5)) {
            let known_definitions = known_definitions(&t);
            let mut builder = TypespaceForGenerate::builder(&t, known_definitions.clone());

            for (ref_, ty) in t.refs_with_types() {
                if known_definitions.contains(&ref_) {
                    builder.add_definition(ref_).unwrap();
                } else {
                    builder.add_use(ty).unwrap();
                }
            }
        }
    }

    #[test]
    fn test_collapses_chains() {
        let mut t = Typespace::default();
        let def = t.add(AlgebraicType::product([("a", AlgebraicType::U32)]));
        let ref1 = t.add(AlgebraicType::Ref(def));
        let ref2 = t.add(AlgebraicType::Ref(ref1));
        let ref3 = t.add(AlgebraicType::Ref(ref2));

        let mut for_generate_forward = TypespaceForGenerate::builder(&t, [def]);
        for_generate_forward.add_definition(def).unwrap();
        let use1 = for_generate_forward.add_use(&ref1.into()).unwrap();
        let use2 = for_generate_forward.add_use(&ref2.into()).unwrap();
        let use3 = for_generate_forward.add_use(&ref3.into()).unwrap();

        assert_eq!(use1, AlgebraicTypeUse::Ref(def));
        assert_eq!(use2, AlgebraicTypeUse::Ref(def));
        assert_eq!(use3, AlgebraicTypeUse::Ref(def));

        let mut for_generate_backward = TypespaceForGenerate::builder(&t, [def]);
        let use3 = for_generate_forward.add_use(&ref3.into()).unwrap();
        let use2 = for_generate_forward.add_use(&ref2.into()).unwrap();
        let use1 = for_generate_forward.add_use(&ref1.into()).unwrap();
        for_generate_backward.add_definition(def).unwrap();

        assert_eq!(use1, AlgebraicTypeUse::Ref(def));
        assert_eq!(use2, AlgebraicTypeUse::Ref(def));
        assert_eq!(use3, AlgebraicTypeUse::Ref(def));
    }

    #[test]
    fn test_detects_cycles() {
        let cyclic_1 = Typespace::new(vec![AlgebraicType::Ref(AlgebraicTypeRef(0))]);
        let mut for_generate = TypespaceForGenerate::builder(&cyclic_1, []);
        let err1 = for_generate.add_use(&AlgebraicType::Ref(AlgebraicTypeRef(0)));

        expect_error_matching!(
            err1,
            ClientCodegenError::TypeRefError(TypeRefError::RecursiveTypeRef(AlgebraicTypeRef(0)))
        );

        let cyclic_2 = Typespace::new(vec![
            AlgebraicType::Ref(AlgebraicTypeRef(1)),
            AlgebraicType::Ref(AlgebraicTypeRef(0)),
        ]);
        let mut for_generate = TypespaceForGenerate::builder(&cyclic_2, []);
        let err1 = for_generate.add_use(&AlgebraicType::Ref(AlgebraicTypeRef(0)));

        expect_error_matching!(
            err1,
            ClientCodegenError::TypeRefError(TypeRefError::RecursiveTypeRef(AlgebraicTypeRef(0)))
        );

        let cyclic_3 = Typespace::new(vec![
            AlgebraicType::Ref(AlgebraicTypeRef(1)),
            AlgebraicType::product([("field", AlgebraicType::Ref(AlgebraicTypeRef(0)))]),
        ]);
        let mut for_generate = TypespaceForGenerate::builder(&cyclic_3, [AlgebraicTypeRef(1)]);
        let err1 = for_generate.add_use(&AlgebraicType::Ref(AlgebraicTypeRef(0)));

        expect_error_matching!(
            err1,
            ClientCodegenError::TypeRefError(TypeRefError::RecursiveTypeRef(AlgebraicTypeRef(0)))
        );
    }
}
