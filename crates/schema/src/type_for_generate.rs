//! `AlgebraicType` extensions for generating client code.

use crate::{
    error::{IdentifierError, PrettyAlgebraicType},
    identifier::Identifier,
};
use enum_as_inner::EnumAsInner;
use petgraph::{
    algo::tarjan_scc,
    visit::{GraphBase, IntoNeighbors, IntoNodeIdentifiers, NodeIndexable},
};
use smallvec::SmallVec;
use spacetimedb_data_structures::{
    error_stream::{CollectAllErrors, CombineErrors, ErrorStream},
    map::{HashMap, HashSet},
};
use spacetimedb_lib::{AlgebraicType, ProductTypeElement};
use spacetimedb_sats::{
    layout::PrimitiveType, typespace::TypeRefError, AlgebraicTypeRef, ArrayType, SumTypeVariant, Typespace,
};
use std::{cell::RefCell, ops::Index, sync::Arc};

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
    NamelessTypeDefElement { ty: PrettyAlgebraicType },

    #[error("internal codegen error: all reducer parameters require names")]
    NamelessReducerParam,

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
///
/// For example, the input:
/// ```txt
/// [
///     0 -> AlgebraicType::Product { a: Ref(1) }
///     1 -> AlgebraicType::Array(Ref(2))
///     2 -> AlgebraicType::Product { b: U32 }
/// ]
/// ```
/// Results in the output:
/// ```txt
/// [
///     0 -> AlgebraicTypeDef::Product { a: Array(Ref(2)) }
///     2 -> AlgebraicTypeDef::Product { b: U32 }
/// ]
/// ```
///
/// Cycles passing through a definition, such as:
/// ```txt
/// [
///     0 -> Product { a: Ref(1) }
///     1 -> Sum { a: U32, b: Ref(0) }
/// ]
/// ```
/// are permitted.
///
/// Cycles NOT passing through a definition, such as:
/// ```txt
/// [
///     0 -> Ref(1)
///     1 -> Array(Ref(0))
/// ]
/// ```
/// are forbidden. (Because most languages do not support anonymous recursive types.)
#[derive(Debug, Clone)]
pub struct TypespaceForGenerate {
    defs: HashMap<AlgebraicTypeRef, AlgebraicTypeDef>,
}

impl TypespaceForGenerate {
    /// Build a `TypespaceForGenerate`.
    ///
    /// We're required to declare known definitions up front.
    /// This is required for distinguishing between a use of the unit type, and a reference to a type declaration of a product type with no elements.
    pub fn builder(
        typespace: &Typespace,
        is_def: impl IntoIterator<Item = AlgebraicTypeRef>,
    ) -> TypespaceForGenerateBuilder<'_> {
        TypespaceForGenerateBuilder {
            typespace,
            result: TypespaceForGenerate {
                defs: HashMap::default(),
            },
            is_def: is_def.into_iter().collect(),
            uses: HashSet::default(),
            known_uses: HashMap::default(),
            currently_touching: HashSet::default(),
        }
    }

    /// Get the definitions of the typespace.
    pub fn defs(&self) -> &HashMap<AlgebraicTypeRef, AlgebraicTypeDef> {
        &self.defs
    }

    /// Get a definition in the typespace.
    pub fn get(&self, ref_: AlgebraicTypeRef) -> Option<&AlgebraicTypeDef> {
        self.defs.get(&ref_)
    }
}

impl Index<AlgebraicTypeRef> for TypespaceForGenerate {
    type Output = AlgebraicTypeDef;

    fn index(&self, index: AlgebraicTypeRef) -> &Self::Output {
        &self.defs[&index]
    }
}
impl Index<&'_ AlgebraicTypeRef> for TypespaceForGenerate {
    type Output = AlgebraicTypeDef;

    fn index(&self, index: &'_ AlgebraicTypeRef) -> &Self::Output {
        &self.defs[index]
    }
}

/// An algebraic type definition.
#[derive(Debug, Clone, PartialEq, Eq, EnumAsInner)]
pub enum AlgebraicTypeDef {
    /// A product type declaration.
    Product(ProductTypeDef),
    /// A sum type declaration.
    Sum(SumTypeDef),
    /// A plain enum definition.
    PlainEnum(PlainEnumTypeDef),
}

thread_local! {
    /// Used to efficiently extract refs from a def.
    static EXTRACT_REFS_BUF: RefCell<HashSet<AlgebraicTypeRef>> = RefCell::new(HashSet::default());
}

impl AlgebraicTypeDef {
    /// Check if a def is recursive.
    pub fn is_recursive(&self) -> bool {
        match self {
            AlgebraicTypeDef::Product(ProductTypeDef { recursive, .. }) => *recursive,
            AlgebraicTypeDef::Sum(SumTypeDef { recursive, .. }) => *recursive,
            AlgebraicTypeDef::PlainEnum(_) => false,
        }
    }

    /// Extract all `AlgebraicTypeRef`s that are used in this type into a buffer.
    /// The buffer may be in arbitrary order, but will not contain duplicates.
    fn extract_refs(&self) -> SmallVec<[AlgebraicTypeRef; 16]> {
        EXTRACT_REFS_BUF.with_borrow_mut(|buf| {
            buf.clear();
            match self {
                AlgebraicTypeDef::Product(ProductTypeDef { elements, .. }) => {
                    for (_, use_) in elements.iter() {
                        use_.extract_refs(buf);
                    }
                }
                AlgebraicTypeDef::Sum(SumTypeDef { variants, .. }) => {
                    for (_, use_) in variants.iter() {
                        use_.extract_refs(buf);
                    }
                }
                AlgebraicTypeDef::PlainEnum(_) => {}
            }
            buf.drain().collect()
        })
    }

    /// Mark a def recursive.
    /// Panics if the def is a `PlainEnum`, because how would that be recursive?
    fn mark_recursive(&mut self) {
        match self {
            AlgebraicTypeDef::Product(ProductTypeDef { recursive, .. }) => {
                *recursive = true;
            }
            AlgebraicTypeDef::Sum(SumTypeDef { recursive, .. }) => {
                *recursive = true;
            }
            AlgebraicTypeDef::PlainEnum(def) => {
                panic!("mark_recursive called on a PlainEnumTypeDef: {def:?}");
            }
        }
    }
}

/// A product type definition.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProductTypeDef {
    /// The elements of the product type, in order.
    pub elements: Box<[(Identifier, AlgebraicTypeUse)]>,
    /// If the type is recursive, that is, contains a use of itself.
    pub recursive: bool,
}

impl<'a> IntoIterator for &'a ProductTypeDef {
    type Item = &'a (Identifier, AlgebraicTypeUse);
    type IntoIter = std::slice::Iter<'a, (Identifier, AlgebraicTypeUse)>;
    fn into_iter(self) -> Self::IntoIter {
        self.elements.iter()
    }
}

/// A sum type definition.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SumTypeDef {
    /// The variants of the sum type, in order.
    pub variants: Box<[(Identifier, AlgebraicTypeUse)]>,
    /// If the type is recursive, that is, contains a use of itself.
    pub recursive: bool,
}

/// A sum type, all of whose variants contain ().
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PlainEnumTypeDef {
    pub variants: Box<[Identifier]>,
}

impl<'a> IntoIterator for &'a SumTypeDef {
    type Item = &'a (Identifier, AlgebraicTypeUse);
    type IntoIter = std::slice::Iter<'a, (Identifier, AlgebraicTypeUse)>;
    fn into_iter(self) -> Self::IntoIter {
        self.variants.iter()
    }
}

/// A use of an algebraic type.
///
/// This type uses `Arc`s to make cloning cheap.
/// These `Arc`s are interned/hash-consed in the `TypespaceForGenerateBuilder`.
/// They are not semantically meaningful and are guaranteed to be acyclic.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum AlgebraicTypeUse {
    /// A type where the definition is given by the typing context (`Typespace`).
    /// In other words, this is defined by a pointer to another `AlgebraicType`.
    /// An AlgebraicTypeUse must point to an `AlgebraicTypeDef`.
    Ref(AlgebraicTypeRef),

    /// The type of array values where elements are of a base type `elem_ty`.
    /// Values [`AlgebraicValue::Array(array)`](crate::AlgebraicValue::Array) will have this type.
    Array(Arc<AlgebraicTypeUse>),

    /// A standard structural option type.
    Option(Arc<AlgebraicTypeUse>),

    /// The special `ScheduleAt` type.
    ScheduleAt,

    /// The special `Identity` type.
    Identity,

    /// The special `ConnectionId` type.
    ConnectionId,

    /// The special `Timestamp` type.
    Timestamp,

    /// The special `TimeDuration` type.
    TimeDuration,

    /// The unit type (empty product).
    /// This is *distinct* from a use of a definition of a product type with no elements.
    Unit,

    /// The never type (empty sum).
    /// This is *distinct* from a use of a definition of a sum type with no variants.
    Never,

    /// The UTF-8 encoded `String` type.
    String,

    /// A primitive type.
    Primitive(PrimitiveType),
}

impl AlgebraicTypeUse {
    /// Extract all `AlgebraicTypeRef`s that are used in this type and add them to `buf`.`
    fn extract_refs(&self, buf: &mut HashSet<AlgebraicTypeRef>) {
        self.for_each_ref(|r| {
            buf.insert(r);
        })
    }

    /// Recurse through this `AlgebraicTypeUse`, calling `f` on every type ref encountered.
    pub fn for_each_ref(&self, mut f: impl FnMut(AlgebraicTypeRef)) {
        self._for_each_ref(&mut f)
    }

    fn _for_each_ref(&self, f: &mut impl FnMut(AlgebraicTypeRef)) {
        match self {
            AlgebraicTypeUse::Ref(ref_) => f(*ref_),
            AlgebraicTypeUse::Array(elem_ty) => elem_ty._for_each_ref(f),
            AlgebraicTypeUse::Option(elem_ty) => elem_ty._for_each_ref(f),
            _ => {}
        }
    }
}

/// A builder for a `TypespaceForGenerate`.
///
/// This is complicated by the fact that a typespace can store both *uses* and *definitions* of types.
pub struct TypespaceForGenerateBuilder<'a> {
    /// The original typespace.
    typespace: &'a Typespace,

    /// The result we are building.
    /// Invariant: all `Def`s in here have been fully processed and correctly marked cyclic.
    /// Not all `Def`s may have been processed yet.
    result: TypespaceForGenerate,

    /// The AlgebraicTypeRefs that we know point to definitions. Must be declared at the start of building.
    /// This is necessary to disambiguate between a use of the unit type, and a reference to a type declaration of a product type with no elements.
    is_def: HashSet<AlgebraicTypeRef>,

    /// Interning data structure, no semantic meaning.
    /// We only intern AlgebraicTypes that are used inside other AlgebraicTypes.
    uses: HashSet<Arc<AlgebraicTypeUse>>,

    /// AlgebraicTypeRefs that point to uses.
    known_uses: HashMap<AlgebraicTypeRef, AlgebraicTypeUse>,

    /// Stores all `AlgebraicTypeRef`s that are currently being operated on.
    currently_touching: HashSet<AlgebraicTypeRef>,
}

impl TypespaceForGenerateBuilder<'_> {
    /// Finish building the `TypespaceForGenerate`.
    /// Panics if `add_definition` has not been called for all of `is_def`.
    pub fn finish(mut self) -> TypespaceForGenerate {
        // Finish validating any straggler uses that weren't already processed.
        for type_ in self.is_def.iter() {
            assert!(
                self.result.defs.contains_key(type_),
                "internal codegen error: not all definitions were processed.
                 Did you call `add_definition` for all types in `is_def`?"
            );
        }

        self.mark_allowed_cycles();

        self.result
    }

    /// Use the `TypespaceForGenerateBuilder` to validate an `AlgebraicTypeUse`.
    /// Does not actually add anything to the `TypespaceForGenerate`.
    pub fn parse_use(&mut self, ty: &AlgebraicType) -> Result<AlgebraicTypeUse> {
        if ty.is_connection_id() {
            Ok(AlgebraicTypeUse::ConnectionId)
        } else if ty.is_identity() {
            Ok(AlgebraicTypeUse::Identity)
        } else if ty.is_timestamp() {
            Ok(AlgebraicTypeUse::Timestamp)
        } else if ty.is_time_duration() {
            Ok(AlgebraicTypeUse::TimeDuration)
        } else if ty.is_unit() {
            Ok(AlgebraicTypeUse::Unit)
        } else if ty.is_never() {
            Ok(AlgebraicTypeUse::Never)
        } else if let Some(elem_ty) = ty.as_option() {
            let elem_ty = self.parse_use(elem_ty)?;
            let interned = self.intern_use(elem_ty);
            Ok(AlgebraicTypeUse::Option(interned))
        } else if ty.is_schedule_at() {
            Ok(AlgebraicTypeUse::ScheduleAt)
        } else {
            match ty {
                AlgebraicType::Ref(ref_) => {
                    // Indirectly recurse.
                    self.parse_ref(*ref_)
                }
                AlgebraicType::Array(ArrayType { elem_ty }) => {
                    let elem_ty = self.parse_use(elem_ty)?;
                    let interned = self.intern_use(elem_ty);
                    Ok(AlgebraicTypeUse::Array(interned))
                }
                AlgebraicType::String => Ok(AlgebraicTypeUse::String),
                AlgebraicType::Bool => Ok(AlgebraicTypeUse::Primitive(PrimitiveType::Bool)),
                AlgebraicType::I8 => Ok(AlgebraicTypeUse::Primitive(PrimitiveType::I8)),
                AlgebraicType::U8 => Ok(AlgebraicTypeUse::Primitive(PrimitiveType::U8)),
                AlgebraicType::I16 => Ok(AlgebraicTypeUse::Primitive(PrimitiveType::I16)),
                AlgebraicType::U16 => Ok(AlgebraicTypeUse::Primitive(PrimitiveType::U16)),
                AlgebraicType::I32 => Ok(AlgebraicTypeUse::Primitive(PrimitiveType::I32)),
                AlgebraicType::U32 => Ok(AlgebraicTypeUse::Primitive(PrimitiveType::U32)),
                AlgebraicType::I64 => Ok(AlgebraicTypeUse::Primitive(PrimitiveType::I64)),
                AlgebraicType::U64 => Ok(AlgebraicTypeUse::Primitive(PrimitiveType::U64)),
                AlgebraicType::I128 => Ok(AlgebraicTypeUse::Primitive(PrimitiveType::I128)),
                AlgebraicType::U128 => Ok(AlgebraicTypeUse::Primitive(PrimitiveType::U128)),
                AlgebraicType::I256 => Ok(AlgebraicTypeUse::Primitive(PrimitiveType::I256)),
                AlgebraicType::U256 => Ok(AlgebraicTypeUse::Primitive(PrimitiveType::U256)),
                AlgebraicType::F32 => Ok(AlgebraicTypeUse::Primitive(PrimitiveType::F32)),
                AlgebraicType::F64 => Ok(AlgebraicTypeUse::Primitive(PrimitiveType::F64)),
                ty @ (AlgebraicType::Product(_) | AlgebraicType::Sum(_)) => {
                    Err(ErrorStream::from(ClientCodegenError::NonSpecialTypeNotAUse {
                        ty: PrettyAlgebraicType(ty.clone()),
                    }))
                }
            }
        }
    }

    /// This is the only seriously complicated case of `parse_use`, which has to deal with cycle detection.
    /// So it gets its own function.
    /// Mutually recursive with `parse_use`.
    fn parse_ref(&mut self, ref_: AlgebraicTypeRef) -> Result<AlgebraicTypeUse> {
        if self.is_def.contains(&ref_) {
            // We know this type is going to be a definition.
            // So, we can just return a ref to it.
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

            if self.currently_touching.contains(&ref_) {
                return Err(ClientCodegenError::TypeRefError(TypeRefError::RecursiveTypeRef(ref_)).into());
            }

            // Mark this ref.
            self.currently_touching.insert(ref_);
            // Recurse.
            let result = self.parse_use(def);
            // Unmark this ref before dealing with possible errors.
            self.currently_touching.remove(&ref_);

            let use_ = result?;

            self.known_uses.insert(ref_, use_.clone());

            Ok(use_)
        }
    }

    /// Add a definition.
    /// Not mutually recursive with anything.
    /// Does not detect cycles, those are left for `mark_allowed_cycles`, which is called after all definitions are processed.
    ///
    /// Why not invoke this for all definitions ourselves, since we know which refs are definitions?
    /// It's so that the caller can wrap errors with better context information.
    pub fn add_definition(&mut self, ref_: AlgebraicTypeRef) -> Result<()> {
        assert!(
            self.is_def.contains(&ref_),
            "internal codegen error: any AlgebraicTypeRef passed to `add_definition` must refer to a declared definition, {ref_} does not"
        );

        let def = self
            .typespace
            .get(ref_)
            .ok_or_else(|| ErrorStream::from(ClientCodegenError::TypeRefError(TypeRefError::InvalidTypeRef(ref_))))?;

        let result = match def {
            AlgebraicType::Product(product) => product
                .elements
                .iter()
                .map(|ProductTypeElement { name, algebraic_type }| self.process_element(def, name, algebraic_type))
                .collect_all_errors()
                .map(|elements| {
                    // We have just processed all the elements, so we know if it's recursive.
                    self.result.defs.insert(
                        ref_,
                        AlgebraicTypeDef::Product(ProductTypeDef {
                            elements,
                            recursive: false, // set in `mark_allowed_cycles`
                        }),
                    );
                }),
            AlgebraicType::Sum(sum) => sum
                .variants
                .iter()
                .map(|SumTypeVariant { name, algebraic_type }| self.process_element(def, name, algebraic_type))
                .collect_all_errors::<Vec<_>>()
                .map(|variants| {
                    if variants.iter().all(|(_, ty)| ty == &AlgebraicTypeUse::Unit) {
                        // We have just processed all the elements, so we know if it's recursive.
                        let variants = variants.into_iter().map(|(name, _)| name).collect();
                        self.result
                            .defs
                            .insert(ref_, AlgebraicTypeDef::PlainEnum(PlainEnumTypeDef { variants }));
                    } else {
                        let variants = variants.into_boxed_slice();

                        self.result.defs.insert(
                            ref_,
                            AlgebraicTypeDef::Sum(SumTypeDef {
                                variants,
                                recursive: false, // set in `mark_allowed_cycles`
                            }),
                        );
                    }
                }),
            _ => Err(ClientCodegenError::NotValidForDefinition {
                ty: PrettyAlgebraicType(def.clone()),
            }
            .into()),
        };

        result
    }

    /// Process an element/variant of a product/sum type.
    ///
    /// `def` is the *containing* type that corresponds to a `Def`,
    /// `algebraic_type` is the type of the element/variant inside `def` and corresponds to a `Use`.
    fn process_element(
        &mut self,
        def: &AlgebraicType,
        element_name: &Option<Box<str>>,
        element_type: &AlgebraicType,
    ) -> Result<(Identifier, AlgebraicTypeUse)> {
        let element_name = element_name
            .as_ref()
            .ok_or_else(|| ErrorStream::from(ClientCodegenError::NamelessTypeDefElement { ty: def.clone().into() }))
            .and_then(|element_name| {
                Identifier::new(element_name.clone()).map_err(|err| {
                    ErrorStream::from(ClientCodegenError::NotValidIdentifier {
                        ty: def.clone().into(),
                        err,
                    })
                })
            });
        let ty = self.parse_use(element_type);
        (element_name, ty).combine_errors()
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

    /// Cycles passing through definitions are allowed.
    /// This function is called after all definitions have been processed.
    fn mark_allowed_cycles(&mut self) {
        let strongly_connected_components: Vec<Vec<AlgebraicTypeRef>> = tarjan_scc(&*self);
        for component in strongly_connected_components {
            if component.len() == 1 {
                // petgraph's implementation returns a vector for all nodes, not distinguishing between
                // self referential and non-self-referential nodes. ignore this for now.
                continue;
            }
            for ref_ in component {
                self.result
                    .defs
                    .get_mut(&ref_)
                    .expect("all defs should be processed by now")
                    .mark_recursive();
            }
        }

        // Now, fix up directly self-referential nodes.
        for (ref_, def_) in &mut self.result.defs {
            let ref_ = *ref_;
            if def_.is_recursive() {
                continue;
            }
            let refs = def_.extract_refs();
            if refs.contains(&ref_) {
                def_.mark_recursive();
            }
        }
    }
}

// We implement some `petgraph` traits for `TypespaceForGenerate` to allow using
// petgraph's implementation of Tarjan's strongly-connected-components algorithm.
// This is used in `mark_allowed_cycles`.
// We don't implement all the traits, only the ones we need.
// The traits are intended to be used *after* all defs have been processed.

impl GraphBase for TypespaceForGenerateBuilder<'_> {
    /// Specifically, definition IDs.
    type NodeId = AlgebraicTypeRef;

    /// Definition `.0` uses definition `.1`.
    type EdgeId = (AlgebraicTypeRef, AlgebraicTypeRef);
}
impl NodeIndexable for TypespaceForGenerateBuilder<'_> {
    fn node_bound(&self) -> usize {
        self.typespace.types.len()
    }

    fn to_index(&self, a: Self::NodeId) -> usize {
        a.idx()
    }

    fn from_index(&self, i: usize) -> Self::NodeId {
        AlgebraicTypeRef(i as _)
    }
}
impl<'a> IntoNodeIdentifiers for &'a TypespaceForGenerateBuilder<'a> {
    type NodeIdentifiers = std::iter::Cloned<hashbrown::hash_set::Iter<'a, spacetimedb_sats::AlgebraicTypeRef>>;

    fn node_identifiers(self) -> Self::NodeIdentifiers {
        self.is_def.iter().cloned()
    }
}
impl<'a> IntoNeighbors for &'a TypespaceForGenerateBuilder<'a> {
    type Neighbors = <SmallVec<[AlgebraicTypeRef; 16]> as IntoIterator>::IntoIter;

    fn neighbors(self, a: Self::NodeId) -> Self::Neighbors {
        self.result
            .defs
            .get(&a)
            .expect("all defs should have been processed by now")
            .extract_refs()
            .into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use spacetimedb_data_structures::expect_error_matching;
    use spacetimedb_lib::AlgebraicType;
    use spacetimedb_sats::proptest::generate_typespace_valid_for_codegen;

    fn is_def(typespace: &Typespace) -> HashSet<AlgebraicTypeRef> {
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
            let is_def = is_def(&t);
            let mut builder = TypespaceForGenerate::builder(&t, is_def.clone());

            for (ref_, ty) in t.refs_with_types() {
                if is_def.contains(&ref_) {
                    builder.add_definition(ref_).unwrap();
                } else {
                    builder.parse_use(ty).unwrap();
                }
            }
        }
    }

    #[test]
    fn test_collapses_chains() {
        let mut t = Typespace::default();
        let def = t.add(AlgebraicType::product([("a", AlgebraicType::U32)]));
        let ref0 = t.add(AlgebraicType::Ref(def));
        let ref1 = t.add(AlgebraicType::array(AlgebraicType::Ref(def)));
        let ref2 = t.add(AlgebraicType::option(AlgebraicType::Ref(ref1)));
        let ref3 = t.add(AlgebraicType::Ref(ref2));

        let expected_0 = AlgebraicTypeUse::Ref(def);
        let expected_1 = AlgebraicTypeUse::Array(Arc::new(expected_0.clone()));
        let expected_2 = AlgebraicTypeUse::Option(Arc::new(expected_1.clone()));
        let expected_3 = expected_2.clone();

        let mut for_generate_forward = TypespaceForGenerate::builder(&t, [def]);
        for_generate_forward.add_definition(def).unwrap();
        let use0 = for_generate_forward.parse_use(&ref0.into()).unwrap();
        let use1 = for_generate_forward.parse_use(&ref1.into()).unwrap();
        let use2 = for_generate_forward.parse_use(&ref2.into()).unwrap();
        let use3 = for_generate_forward.parse_use(&ref3.into()).unwrap();

        assert_eq!(use0, expected_0);
        assert_eq!(use1, expected_1);
        assert_eq!(use2, expected_2);
        assert_eq!(use3, expected_3);

        let mut for_generate_backward = TypespaceForGenerate::builder(&t, [def]);
        let use3 = for_generate_forward.parse_use(&ref3.into()).unwrap();
        let use2 = for_generate_forward.parse_use(&ref2.into()).unwrap();
        let use1 = for_generate_forward.parse_use(&ref1.into()).unwrap();
        let use0 = for_generate_backward.parse_use(&ref0.into()).unwrap();
        for_generate_backward.add_definition(def).unwrap();

        assert_eq!(use0, expected_0);
        assert_eq!(use1, expected_1);
        assert_eq!(use2, expected_2);
        assert_eq!(use3, expected_3);
    }

    #[test]
    fn test_detects_cycles_1() {
        let cyclic_1 = Typespace::new(vec![AlgebraicType::Ref(AlgebraicTypeRef(0))]);
        let mut for_generate = TypespaceForGenerate::builder(&cyclic_1, []);
        let err1 = for_generate.parse_use(&AlgebraicType::Ref(AlgebraicTypeRef(0)));

        expect_error_matching!(
            err1,
            ClientCodegenError::TypeRefError(TypeRefError::RecursiveTypeRef(AlgebraicTypeRef(0)))
        );
    }

    #[test]
    fn test_detects_cycles_2() {
        let cyclic_2 = Typespace::new(vec![
            AlgebraicType::Ref(AlgebraicTypeRef(1)),
            AlgebraicType::Ref(AlgebraicTypeRef(0)),
        ]);
        let mut for_generate = TypespaceForGenerate::builder(&cyclic_2, []);
        let err2 = for_generate.parse_use(&AlgebraicType::Ref(AlgebraicTypeRef(0)));

        expect_error_matching!(
            err2,
            ClientCodegenError::TypeRefError(TypeRefError::RecursiveTypeRef(AlgebraicTypeRef(0)))
        );
    }

    #[test]
    fn test_detects_cycles_3() {
        let cyclic_3 = Typespace::new(vec![
            AlgebraicType::Ref(AlgebraicTypeRef(1)),
            AlgebraicType::product([("field", AlgebraicType::Ref(AlgebraicTypeRef(0)))]),
        ]);
        let mut for_generate = TypespaceForGenerate::builder(&cyclic_3, [AlgebraicTypeRef(1)]);
        for_generate
            .parse_use(&AlgebraicType::Ref(AlgebraicTypeRef(0)))
            .expect("should be allowed");
        for_generate
            .add_definition(AlgebraicTypeRef(1))
            .expect("should be allowed");
        let result = for_generate.finish();
        let table = result.defs().get(&AlgebraicTypeRef(1)).expect("should be defined");

        assert!(table.is_recursive(), "recursion not detected? table: {table:?}");
    }

    #[test]
    fn test_detects_cycles_4() {
        let cyclic_4 = Typespace::new(vec![
            AlgebraicType::product([("field", AlgebraicTypeRef(1).into())]),
            AlgebraicType::product([("field", AlgebraicTypeRef(2).into())]),
            AlgebraicType::product([("field", AlgebraicTypeRef(3).into())]),
            AlgebraicType::product([("field", AlgebraicTypeRef(0).into())]),
            AlgebraicType::product([("field", AlgebraicTypeRef(0).into())]),
        ]);
        let mut for_generate = TypespaceForGenerate::builder(
            &cyclic_4,
            [
                AlgebraicTypeRef(0),
                AlgebraicTypeRef(1),
                AlgebraicTypeRef(2),
                AlgebraicTypeRef(3),
                AlgebraicTypeRef(4),
            ],
        );

        for i in 0..5 {
            for_generate
                .parse_use(&AlgebraicType::Ref(AlgebraicTypeRef(i)))
                .expect("should be allowed");
            for_generate
                .add_definition(AlgebraicTypeRef(i))
                .expect("should be allowed");
        }
        let result = for_generate.finish();

        for i in 0..4 {
            assert!(result[AlgebraicTypeRef(i)].is_recursive(), "recursion not detected");
        }
        assert!(
            !result[AlgebraicTypeRef(4)].is_recursive(),
            "recursion detected incorrectly"
        );
    }

    #[test]
    fn test_detects_cycles_5() {
        // Branching cycles.
        let cyclic_5 = Typespace::new(vec![
            // cyclic component.
            AlgebraicType::product([("field", AlgebraicTypeRef(1).into())]),
            AlgebraicType::product([
                ("cyclic_1", AlgebraicTypeRef(2).into()),
                ("cyclic_2", AlgebraicTypeRef(3).into()),
                ("acyclic", AlgebraicTypeRef(5).into()),
            ]),
            AlgebraicType::product([("field", AlgebraicTypeRef(0).into())]),
            AlgebraicType::product([("field", AlgebraicTypeRef(0).into())]),
            // points into cyclic component, but is not cyclic.
            AlgebraicType::product([("field", AlgebraicTypeRef(2).into())]),
            // referred to by cyclic component, but is not cyclic.
            AlgebraicType::product([("field", AlgebraicType::U32)]),
        ]);
        let mut for_generate = TypespaceForGenerate::builder(
            &cyclic_5,
            [
                AlgebraicTypeRef(0),
                AlgebraicTypeRef(1),
                AlgebraicTypeRef(2),
                AlgebraicTypeRef(3),
                AlgebraicTypeRef(4),
                AlgebraicTypeRef(5),
            ],
        );

        for i in 0..6 {
            for_generate
                .parse_use(&AlgebraicType::Ref(AlgebraicTypeRef(i)))
                .expect("should be allowed");
            for_generate
                .add_definition(AlgebraicTypeRef(i))
                .expect("should be allowed");
        }
        let result = for_generate.finish();

        for i in 0..4 {
            assert!(result[AlgebraicTypeRef(i)].is_recursive(), "recursion not detected");
        }
        for i in 4..6 {
            assert!(
                !result[AlgebraicTypeRef(i)].is_recursive(),
                "recursion detected incorrectly"
            );
        }
    }
}
