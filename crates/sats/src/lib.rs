pub mod algebraic_type;
pub mod builtin_type;
pub mod builtin_value;
pub mod convert;
pub mod product_type;
pub mod product_type_element;
pub mod product_value;
pub mod relation;
pub mod sum_type;
pub mod sum_type_variant;
pub mod sum_value;
pub mod typespace;
// mod algebraic_type_legacy_encoding;
mod algebraic_type_ref;
pub mod algebraic_value;
pub mod bsatn;
pub mod buffer;
pub mod de;
mod resolve_refs;
pub mod satn;
pub mod ser;

pub use algebraic_type::AlgebraicType;
pub use algebraic_type_ref::AlgebraicTypeRef;
pub use algebraic_value::AlgebraicValue;
pub use builtin_type::{BuiltinType, MapType};
pub use builtin_value::BuiltinValue;
pub use product_type::ProductType;
pub use product_type_element::ProductTypeElement;
pub use product_value::ProductValue;
pub use sum_type::SumType;
pub use sum_type_variant::SumTypeVariant;
pub use sum_value::SumValue;
pub use typespace::Typespace;

pub trait Value {
    type Type;
}

pub struct ValueWithType<'a, T: Value> {
    ty: TypeInSpace<'a, T::Type>,
    val: &'a T,
}

impl<T: Value> Copy for ValueWithType<'_, T> {}
impl<T: Value> Clone for ValueWithType<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T: Value> ValueWithType<'a, T> {
    pub fn new(ty: TypeInSpace<'a, T::Type>, val: &'a T) -> Self {
        Self { ty, val }
    }
    pub fn value(&self) -> &'a T {
        self.val
    }
    pub fn ty(&self) -> &'a T::Type {
        self.ty.ty
    }
    pub fn typespace(&self) -> &'a Typespace {
        self.ty.typespace
    }
    pub fn with<'b, U: Value>(&self, ty: &'b U::Type, val: &'b U) -> ValueWithType<'b, U>
    where
        'a: 'b,
    {
        ValueWithType {
            ty: self.ty.with(ty),
            val,
        }
    }
}

pub struct TypeInSpace<'a, T: ?Sized> {
    typespace: &'a Typespace,
    ty: &'a T,
}

impl<T: ?Sized> Copy for TypeInSpace<'_, T> {}
impl<T: ?Sized> Clone for TypeInSpace<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T: ?Sized> TypeInSpace<'a, T> {
    pub fn new(typespace: &'a Typespace, ty: &'a T) -> Self {
        Self { typespace, ty }
    }

    pub fn ty(&self) -> &'a T {
        self.ty
    }

    pub fn typespace(&self) -> &'a Typespace {
        self.typespace
    }

    pub fn with<'b, U>(&self, ty: &'b U) -> TypeInSpace<'b, U>
    where
        'a: 'b,
    {
        TypeInSpace {
            typespace: self.typespace,
            ty,
        }
    }

    pub fn with_value<'b, V: Value<Type = T>>(&self, val: &'b V) -> ValueWithType<'b, V>
    where
        'a: 'b,
    {
        ValueWithType::new(*self, val)
    }

    pub fn resolve(&self, r: AlgebraicTypeRef) -> TypeInSpace<'a, AlgebraicType> {
        TypeInSpace {
            typespace: self.typespace,
            ty: &self.typespace[r],
        }
    }

    pub fn map<U: ?Sized>(&self, f: impl FnOnce(&'a T) -> &'a U) -> TypeInSpace<'a, U> {
        TypeInSpace {
            typespace: self.typespace,
            ty: f(self.ty),
        }
    }
}

struct FDisplay<F>(F);
impl<F: Fn(&mut std::fmt::Formatter) -> std::fmt::Result> std::fmt::Display for FDisplay<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (self.0)(f)
    }
}
impl<F: Fn(&mut std::fmt::Formatter) -> std::fmt::Result> std::fmt::Debug for FDisplay<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (self.0)(f)
    }
}
fn fmt_fn<F: Fn(&mut std::fmt::Formatter) -> std::fmt::Result>(f: F) -> FDisplay<F> {
    FDisplay(f)
}
