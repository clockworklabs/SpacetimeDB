mod impls;
#[cfg(feature = "serde")]
pub mod serde;

#[doc(hidden)]
pub use impls::{visit_named_product, visit_seq_product};

use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;

use crate::{fmt_fn, FDisplay};

pub trait Deserializer<'de>: Sized {
    type Error: Error;

    fn deserialize_product<V: ProductVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error>;

    fn deserialize_sum<V: SumVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error>;

    fn deserialize_bool(self) -> Result<bool, Self::Error>;
    fn deserialize_u8(self) -> Result<u8, Self::Error>;
    fn deserialize_u16(self) -> Result<u16, Self::Error>;
    fn deserialize_u32(self) -> Result<u32, Self::Error>;
    fn deserialize_u64(self) -> Result<u64, Self::Error>;
    fn deserialize_u128(self) -> Result<u128, Self::Error>;
    fn deserialize_i8(self) -> Result<i8, Self::Error>;
    fn deserialize_i16(self) -> Result<i16, Self::Error>;
    fn deserialize_i32(self) -> Result<i32, Self::Error>;
    fn deserialize_i64(self) -> Result<i64, Self::Error>;
    fn deserialize_i128(self) -> Result<i128, Self::Error>;
    fn deserialize_f32(self) -> Result<f32, Self::Error>;
    fn deserialize_f64(self) -> Result<f64, Self::Error>;

    fn deserialize_str<V: SliceVisitor<'de, str>>(self, visitor: V) -> Result<V::Output, Self::Error>;

    fn deserialize_bytes<V: SliceVisitor<'de, [u8]>>(self, visitor: V) -> Result<V::Output, Self::Error>;

    fn deserialize_array<V: ArrayVisitor<'de, T>, T: Deserialize<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Output, Self::Error> {
        self.deserialize_array_seed(visitor, PhantomData)
    }

    fn deserialize_array_seed<V: ArrayVisitor<'de, T::Output>, T: DeserializeSeed<'de> + Clone>(
        self,
        visitor: V,
        seed: T,
    ) -> Result<V::Output, Self::Error>;

    fn deserialize_map<Vi: MapVisitor<'de, K, V>, K: Deserialize<'de>, V: Deserialize<'de>>(
        self,
        visitor: Vi,
    ) -> Result<Vi::Output, Self::Error> {
        self.deserialize_map_seed(visitor, PhantomData, PhantomData)
    }

    fn deserialize_map_seed<
        Vi: MapVisitor<'de, K::Output, V::Output>,
        K: DeserializeSeed<'de> + Clone,
        V: DeserializeSeed<'de> + Clone,
    >(
        self,
        visitor: Vi,
        kseed: K,
        vseed: V,
    ) -> Result<Vi::Output, Self::Error>;
}

pub trait Error: Sized {
    fn custom(msg: impl fmt::Display) -> Self;

    fn invalid_product_length<'de, T: ProductVisitor<'de>>(len: usize, expected: &T) -> Self {
        Self::custom(format_args!(
            "invalid length {}, expected {}",
            len,
            fmt_invalid_len(expected)
        ))
    }

    fn missing_field<'de, T: ProductVisitor<'de>>(field: usize, field_name: Option<&str>, prod: &T) -> Self {
        Self::custom(error_on_field("missing ", field, field_name, prod))
    }

    fn duplicate_field<'de, T: ProductVisitor<'de>>(field: usize, field_name: Option<&str>, prod: &T) -> Self {
        Self::custom(error_on_field("duplicate ", field, field_name, prod))
    }

    fn unknown_field_name<'de, T: FieldNameVisitor<'de>>(field_name: &str, expected: &T) -> Self {
        let el_ty = match expected.kind() {
            ProductKind::Normal => "field",
            ProductKind::ReducerArgs => "reducer argument",
        };
        if let Some(one_of) = one_of_names(|n| expected.field_names(n)) {
            Self::custom(format_args!("unknown {el_ty} `{field_name}`, expected {one_of}",))
        } else {
            Self::custom(format_args!("unknown {el_ty} `{field_name}`, there are no {el_ty}s"))
        }
    }

    fn unknown_variant_tag<'de, T: SumVisitor<'de>>(tag: u8, expected: &T) -> Self {
        Self::custom(format_args!(
            "unknown tag {tag:#x} for sum type {}",
            expected.sum_name().unwrap_or("<sum>"),
        ))
    }

    fn unknown_variant_name<T: VariantVisitor>(name: &str, expected: &T) -> Self {
        if let Some(one_of) = one_of_names(|n| expected.variant_names(n)) {
            Self::custom(format_args!("unknown variant `{name}`, expected {one_of}",))
        } else {
            Self::custom(format_args!("unknown variant `{name}`, there are no variants"))
        }
    }
}

fn error_on_field<'a, 'de, T: ProductVisitor<'de>>(
    problem: &'static str,
    field: usize,
    field_name: Option<&'a str>,
    prod: &T,
) -> impl fmt::Display + 'a {
    let field_kind = match prod.product_kind() {
        ProductKind::Normal => "field",
        ProductKind::ReducerArgs => "reducer argument",
    };
    fmt_fn(move |f| {
        // e.g. "missing field `foo`"
        f.write_str(problem)?;
        f.write_str(field_kind)?;
        if let Some(name) = field_name {
            write!(f, " `{}`", name)
        } else {
            write!(f, " (index {})", field)
        }
    })
}

fn fmt_invalid_len<'de, T: ProductVisitor<'de>>(
    expected: &T,
) -> FDisplay<impl Fn(&mut fmt::Formatter) -> fmt::Result + '_> {
    fmt_fn(|f| {
        let ty = match expected.product_kind() {
            ProductKind::Normal => "product type",
            ProductKind::ReducerArgs => "reducer args for",
        };
        let name = expected.product_name().unwrap_or("<product>");
        let len = expected.product_len();

        write!(f, "{ty} {name} with {len} elements")
    })
}

pub trait ProductVisitor<'de> {
    type Output;

    fn product_name(&self) -> Option<&str>;
    fn product_len(&self) -> usize;
    fn product_kind(&self) -> ProductKind {
        ProductKind::Normal
    }

    fn visit_seq_product<A: SeqProductAccess<'de>>(self, prod: A) -> Result<Self::Output, A::Error>;
    fn visit_named_product<A: NamedProductAccess<'de>>(self, prod: A) -> Result<Self::Output, A::Error>;
}

#[derive(Clone, Copy)]
pub enum ProductKind {
    Normal,
    ReducerArgs,
}

pub trait SeqProductAccess<'de> {
    type Error: Error;

    fn next_element<T: Deserialize<'de>>(&mut self) -> Result<Option<T>, Self::Error> {
        self.next_element_seed(PhantomData)
    }

    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Output>, Self::Error>;
}

pub trait NamedProductAccess<'de> {
    type Error: Error;

    fn get_field_ident<V: FieldNameVisitor<'de>>(&mut self, visitor: V) -> Result<Option<V::Output>, Self::Error>;

    fn get_field_value<T: Deserialize<'de>>(&mut self) -> Result<T, Self::Error> {
        self.get_field_value_seed(PhantomData)
    }

    fn get_field_value_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<T::Output, Self::Error>;
}

pub trait FieldNameVisitor<'de> {
    type Output;

    fn kind(&self) -> ProductKind {
        ProductKind::Normal
    }
    fn field_names(&self, names: &mut dyn ValidNames);

    fn visit<E: Error>(self, name: &str) -> Result<Self::Output, E>;
}

pub trait ValidNames {
    fn push(&mut self, s: &str);
}
impl dyn ValidNames + '_ {
    pub fn extend<I: IntoIterator>(&mut self, i: I)
    where
        I::Item: AsRef<str>,
    {
        for name in i {
            self.push(name.as_ref())
        }
    }
}

pub trait SumVisitor<'de> {
    type Output;

    fn sum_name(&self) -> Option<&str>;

    fn visit_sum<A: SumAccess<'de>>(self, data: A) -> Result<Self::Output, A::Error>;
}

pub trait SumAccess<'de> {
    type Error: Error;
    type Variant: VariantAccess<'de, Error = Self::Error>;

    fn variant<V: VariantVisitor>(self, visitor: V) -> Result<(V::Output, Self::Variant), Self::Error>;
}

pub trait VariantVisitor {
    type Output;

    fn variant_names(&self, names: &mut dyn ValidNames);

    fn visit_tag<E: Error>(self, tag: u8) -> Result<Self::Output, E>;
    fn visit_name<E: Error>(self, name: &str) -> Result<Self::Output, E>;
}

pub trait VariantAccess<'de>: Sized {
    type Error: Error;

    fn deserialize<T: Deserialize<'de>>(self) -> Result<T, Self::Error> {
        self.deserialize_seed(PhantomData)
    }

    fn deserialize_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Output, Self::Error>;
}

pub trait SliceVisitor<'de, T: ToOwned + ?Sized>: Sized {
    type Output;

    fn visit<E: Error>(self, slice: &T) -> Result<Self::Output, E>;

    fn visit_owned<E: Error>(self, buf: T::Owned) -> Result<Self::Output, E> {
        self.visit(buf.borrow())
    }

    fn visit_borrowed<E: Error>(self, borrowed_slice: &'de T) -> Result<Self::Output, E> {
        self.visit(borrowed_slice)
    }
}

pub trait ArrayVisitor<'de, T> {
    type Output;

    fn visit<A: ArrayAccess<'de, Element = T>>(self, vec: A) -> Result<Self::Output, A::Error>;
}

pub trait ArrayAccess<'de> {
    type Element;
    type Error;

    fn next_element(&mut self) -> Result<Option<Self::Element>, Self::Error>;

    fn size_hint(&self) -> Option<usize> {
        None
    }
}

pub trait MapVisitor<'de, K, V> {
    type Output;

    fn visit<A: MapAccess<'de, Key = K, Value = V>>(self, map: A) -> Result<Self::Output, A::Error>;
}

pub trait MapAccess<'de> {
    type Key;
    type Value;
    type Error;

    fn next_entry(&mut self) -> Result<Option<(Self::Key, Self::Value)>, Self::Error>;

    fn size_hint(&self) -> Option<usize> {
        None
    }
}

pub trait DeserializeSeed<'de> {
    type Output;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error>;
}

pub use spacetimedb_bindings_macro::Deserialize;
pub trait Deserialize<'de>: Sized {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error>;

    /// used in the Deserialize for Vec<T> impl to allow specializing deserializing Vec<T> as bytes
    #[doc(hidden)]
    #[inline(always)]
    fn __deserialize_vec<D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Self>, D::Error> {
        deserializer.deserialize_array(BasicVecVisitor)
    }
}

pub trait DeserializeOwned: for<'de> Deserialize<'de> {}
impl<T> DeserializeOwned for T where T: for<'de> Deserialize<'de> {}

impl<'de, T: Deserialize<'de>> DeserializeSeed<'de> for PhantomData<T> {
    type Output = T;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        T::deserialize(deserializer)
    }
}

pub struct BasicVecVisitor;

impl<'de, T> ArrayVisitor<'de, T> for BasicVecVisitor {
    type Output = Vec<T>;

    fn visit<A: ArrayAccess<'de, Element = T>>(self, mut vec: A) -> Result<Self::Output, A::Error> {
        let mut v = Vec::with_capacity(vec.size_hint().unwrap_or(0));
        while let Some(el) = vec.next_element()? {
            v.push(el)
        }
        Ok(v)
    }
}

pub struct BasicMapVisitor;

impl<'de, K: Ord, V> MapVisitor<'de, K, V> for BasicMapVisitor {
    type Output = BTreeMap<K, V>;

    fn visit<A: MapAccess<'de, Key = K, Value = V>>(self, mut map: A) -> Result<Self::Output, A::Error> {
        let mut m = Vec::with_capacity(map.size_hint().unwrap_or(0));
        while let Some(entry) = map.next_entry()? {
            m.push(entry)
        }
        Ok(m.into_iter().collect())
    }
}

fn one_of_names(names: impl Fn(&mut dyn ValidNames)) -> Option<impl fmt::Display> {
    let mut n = NNames(0);
    names(&mut n);
    let NNames(n) = n;
    (n != 0).then(|| {
        fmt_fn(move |f| {
            let mut f = OneOfNames::new(n != 2, f);
            names(&mut f);
            f.f.map(drop)
        })
    })
}

struct NNames(usize);
impl ValidNames for NNames {
    fn push(&mut self, _: &str) {
        self.0 += 1
    }
}

struct OneOfNames<'a, 'b> {
    at_start: bool,
    many: bool,
    f: Result<&'a mut fmt::Formatter<'b>, fmt::Error>,
}
impl<'a, 'b> OneOfNames<'a, 'b> {
    fn new(many: bool, f: &'a mut fmt::Formatter<'b>) -> Self {
        Self {
            at_start: true,
            many,
            f: Ok(f),
        }
    }
}
impl ValidNames for OneOfNames<'_, '_> {
    fn push(&mut self, name: &str) {
        let (start, sep) = if self.many { ("", " or ") } else { ("one of", ", ") };
        if let Ok(f) = &mut self.f {
            let mut go = || -> fmt::Result {
                f.write_str(if std::mem::take(&mut self.at_start) { start } else { sep })?;
                write!(f, "`{name}`")
            };
            if let Err(e) = go() {
                self.f = Err(e);
            }
        }
    }
}
