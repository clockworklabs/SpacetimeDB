use std::any::TypeId;
use std::ops::{Index, IndexMut};

use crate::algebraic_type::AlgebraicType;
use crate::algebraic_type_ref::AlgebraicTypeRef;
use crate::{de::Deserialize, ser::Serialize};
use crate::{ArrayType, BuiltinType, TypeInSpace};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[sats(crate = crate)]
pub struct Typespace {
    pub types: Vec<AlgebraicType>,
}

impl Default for Typespace {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl Index<AlgebraicTypeRef> for Typespace {
    type Output = AlgebraicType;

    fn index(&self, index: AlgebraicTypeRef) -> &Self::Output {
        &self.types[index.0 as usize]
    }
}
impl IndexMut<AlgebraicTypeRef> for Typespace {
    fn index_mut(&mut self, index: AlgebraicTypeRef) -> &mut Self::Output {
        &mut self.types[index.0 as usize]
    }
}

impl Typespace {
    pub const fn new(types: Vec<AlgebraicType>) -> Self {
        Self { types }
    }

    pub fn get(&self, r: AlgebraicTypeRef) -> Option<&AlgebraicType> {
        self.types.get(r.0 as usize)
    }

    /// Generates a fresh "type variable" that is set to `ty` and returns it.
    ///
    /// This allows later changing the meaning of the returned type variable
    /// if for whatever reason, you cannot provide the full definition of the type yet.
    pub fn add(&mut self, ty: AlgebraicType) -> AlgebraicTypeRef {
        let i = self.types.len();
        self.types.push(ty);
        AlgebraicTypeRef(i as u32)
    }

    pub fn with_type<'a, T: ?Sized>(&'a self, ty: &'a T) -> TypeInSpace<'a, T> {
        TypeInSpace::new(self, ty)
    }
}

pub trait SpacetimeType {
    fn make_type<S: TypespaceBuilder>(typespace: &mut S) -> AlgebraicType;
}

pub use spacetimedb_bindings_macro::SpacetimeType;

pub trait TypespaceBuilder {
    fn add(
        &mut self,
        typeid: TypeId,
        name: Option<&'static str>,
        make_ty: impl FnOnce(&mut Self) -> AlgebraicType,
    ) -> AlgebraicType;
}

macro_rules! impl_primitives {
    ($($t:ty => $x:ident,)*) => {
        $(
            impl SpacetimeType for $t {
                fn make_type<S: TypespaceBuilder>(_ts: &mut S) -> AlgebraicType {
                    AlgebraicType::$x
                }
            }
        )*
    };
}

impl_primitives! {
    bool => Bool,
    u8 => U8,
    i8 => I8,
    u16 => U16,
    i16 => I16,
    u32 => U32,
    i32 => I32,
    u64 => U64,
    i64 => I64,
    u128 => U128,
    i128 => I128,
    f32 => F32,
    f64 => F64,
    String => String,
}

impl SpacetimeType for () {
    fn make_type<S: TypespaceBuilder>(_ts: &mut S) -> AlgebraicType {
        AlgebraicType::UNIT_TYPE
    }
}
impl SpacetimeType for &str {
    fn make_type<S: TypespaceBuilder>(_ts: &mut S) -> AlgebraicType {
        AlgebraicType::String
    }
}

impl<T: SpacetimeType> SpacetimeType for Vec<T> {
    fn make_type<S: TypespaceBuilder>(typespace: &mut S) -> AlgebraicType {
        AlgebraicType::Builtin(BuiltinType::Array(ArrayType {
            elem_ty: Box::new(T::make_type(typespace)),
        }))
    }
}

impl<T: SpacetimeType> SpacetimeType for Option<T> {
    fn make_type<S: TypespaceBuilder>(typespace: &mut S) -> AlgebraicType {
        AlgebraicType::make_option_type(T::make_type(typespace))
    }
}
