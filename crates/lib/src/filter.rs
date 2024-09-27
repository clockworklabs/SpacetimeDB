use crate::de::Deserialize;
use crate::operator::{OpCmp, OpLogic, OpUnary};
use crate::ser::Serialize;
use crate::AlgebraicValue;
use spacetimedb_sats::buffer::DecodeError;
use spacetimedb_sats::de::{
    DeserializeSeed, Deserializer, Error, ProductVisitor, SumAccess, SumVisitor, ValidNames, VariantAccess,
    VariantVisitor,
};
use spacetimedb_sats::{ProductTypeElement, Typespace};
use std::fmt::Debug;
use std::marker::PhantomData;

macro_rules! impl_sum {
    ($seed_ty:ident, $ty:ident { $($variant:ident ( $variant_ty:ty ),)* }) => {
        const _: () = {
            #[repr(u8)]
            enum Tag {
                $($variant,)*
            }

            impl VariantVisitor for With<$seed_ty<'_>, $ty> {
                type Output = Tag;

                fn variant_names(&self, names: &mut dyn ValidNames) {
                    names.extend([$(stringify!($variant),)*]);
                }

                fn visit_tag<E: Error>(self, tag: u8) -> Result<Self::Output, E> {
                    $(if tag == Tag::$variant as u8 {
                        return Ok(Tag::$variant);
                    })*
                    Err(E::unknown_variant_tag(tag, &self))
                }

                fn visit_name<E: Error>(self, name: &str) -> Result<Self::Output, E> {
                    match name {
                        $(stringify!($variant) => Ok(Tag::$variant),)*
                        _ => Err(E::unknown_variant_name(name, &self)),
                    }
                }
            }

            impl<'de> SumVisitor<'de> for With<$seed_ty<'_>, $ty> {
                type Output = $ty;

                fn sum_name(&self) -> Option<&str> {
                    Some(stringify!($ty))
                }

                fn visit_sum<A: SumAccess<'de>>(self, data: A) -> Result<Self::Output, A::Error> {
                    let (tag, data) = data.variant(self)?;
                    match tag {
                        $(Tag::$variant => data.deserialize_seed(self.with_type::<$variant_ty>()).map($ty::$variant),)*
                    }
                }
            }

            impl<'de> DeserializeSeed<'de> for With<$seed_ty<'_>, $ty> {
                type Output = $ty;

                fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
                    deserializer.deserialize_sum(self)
                }
            }
        };
    };
}

macro_rules! count {
    ($first:ident $($rest:ident)*) => (1usize + count!($( $rest )*));
    () => (0usize);
}

macro_rules! impl_product {
    (@seed $first_seed:expr $(, $other_seed:expr)?) => ($first_seed);

    ($seed_ty:ident, $ty:ident { $($(#[seed = $seed:expr])? $field:ident: $field_ty:ty),* $(,)? }) => {
        impl<'de> ProductVisitor<'de> for With<$seed_ty<'_>, $ty> {
            type Output = $ty;

            fn product_name(&self) -> Option<&str> {
                Some(stringify!($ty))
            }

            fn product_len(&self) -> usize {
                count!( $($field)* )
            }

            fn visit_seq_product<A: spacetimedb_sats::de::SeqProductAccess<'de>>(
                self,
                mut prod: A,
            ) -> Result<Self::Output, A::Error> {
                let mut index = 0usize;

                $(
                    let $field =
                        prod
                        // TODO: remove the braces around {$seed} when clippy false positive is fixed
                        .next_element_seed(impl_product!(@seed $({$seed}(self.ctx),)? self.with_type::<$field_ty>()))?
                        .ok_or_else(|| {
                            A::Error::missing_field(
                                {
                                    let i = index;
                                    index += 1;
                                    i
                                },
                                Some(stringify!($field)),
                                &self
                            )
                        })?;
                )*

                Ok($ty { $($field),* })
            }

            fn visit_named_product<A: spacetimedb_sats::de::NamedProductAccess<'de>>(
                self,
                _prod: A,
            ) -> Result<Self::Output, A::Error> {
                // Maybe implement later, although we shouldn't need this for filters.
                Err(A::Error::custom("named product not supported"))
            }
        }

        impl<'de> DeserializeSeed<'de> for With<$seed_ty<'_>, $ty> {
            type Output = $ty;

            fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
                deserializer.deserialize_product(self)
            }
        }
    };
}

macro_rules! impl_forward {
    ($ty:ty) => {
        impl<'de, Ctx> DeserializeSeed<'de> for With<Ctx, $ty> {
            type Output = $ty;

            fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
                Deserialize::deserialize(deserializer)
            }
        }
    };
}

#[derive(Clone, Copy)]
struct DeCtx<'types> {
    typespace: &'types Typespace,
    fields: &'types [ProductTypeElement],
}

#[derive(Clone, Copy)]
struct DeCtxWithLhs<'types> {
    inner: DeCtx<'types>,
    lhs_field: u16,
}

struct With<Ctx, T> {
    ctx: Ctx,
    _marker: PhantomData<fn() -> T>,
}

impl<Ctx: Clone, T> Clone for With<Ctx, T> {
    fn clone(&self) -> Self {
        With {
            ctx: self.ctx.clone(),
            _marker: PhantomData,
        }
    }
}

impl<Ctx: Copy, T> Copy for With<Ctx, T> {}

impl<Ctx, T> With<Ctx, T> {
    fn with_type<U>(self) -> With<Ctx, U> {
        With {
            ctx: self.ctx,
            _marker: PhantomData,
        }
    }
}

impl<'de, Ctx, T> DeserializeSeed<'de> for With<Ctx, Box<T>>
where
    With<Ctx, T>: DeserializeSeed<'de, Output = T>,
{
    type Output = Box<T>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        DeserializeSeed::deserialize(self.with_type::<T>(), deserializer).map(Box::new)
    }
}

impl<'de> DeserializeSeed<'de> for With<DeCtxWithLhs<'_>, AlgebraicValue> {
    type Output = AlgebraicValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        let ctx = self.ctx.inner;

        ctx.typespace
            .with_type(
                &ctx.fields
                    .get(self.ctx.lhs_field as usize)
                    .ok_or_else(|| D::Error::custom("field index out of range"))?
                    .algebraic_type,
            )
            .deserialize(deserializer)
    }
}

impl_forward!(u16);

#[derive(Debug, Serialize)]
pub enum Rhs {
    Value(AlgebraicValue),
    Field(u16),
}

impl_sum!(DeCtxWithLhs, Rhs {
    Value(AlgebraicValue),
    Field(u16),
});

// This type can only be (de)serialized as part of [`Cmp`]
// as the AlgebraicValue needs to be the same type as LHS
// and isn't self-describing.
#[derive(Debug, Serialize)]
pub struct CmpArgs {
    pub lhs_field: u16,
    pub rhs: Rhs,
}

impl_product!(
    DeCtx,
    CmpArgs {
        lhs_field: u16,

        #[seed = |ctx| With::<_, Rhs> {
            ctx: DeCtxWithLhs { inner: ctx, lhs_field },
            _marker: PhantomData,
        }]
        rhs: Rhs,
    }
);

impl_forward!(OpCmp);

#[derive(Debug, Serialize)]
pub struct Cmp {
    pub op: OpCmp,
    pub args: CmpArgs,
}

impl_product!(
    DeCtx,
    Cmp {
        op: OpCmp,
        args: CmpArgs
    }
);

impl_forward!(OpLogic);

#[derive(Debug, Serialize)]
pub struct Logic {
    pub lhs: Box<Expr>,
    pub op: OpLogic,
    pub rhs: Box<Expr>,
}

impl_product!(
    DeCtx,
    Logic {
        lhs: Box<Expr>,
        op: OpLogic,
        rhs: Box<Expr>
    }
);

impl_forward!(OpUnary);

#[derive(Debug, Serialize)]
pub struct Unary {
    pub op: OpUnary,
    pub arg: Box<Expr>,
}

impl_product!(DeCtx, Unary { op: OpUnary, arg: Box<Expr> });

#[derive(Debug, Serialize)]
pub enum Expr {
    Cmp(Cmp),
    Logic(Logic),
    Unary(Unary),
}

impl_sum!(DeCtx, Expr {
    Cmp(Cmp),
    Logic(Logic),
    Unary(Unary),
});

impl Expr {
    pub fn from_bytes(
        typespace: &Typespace,
        fields: &[ProductTypeElement],
        mut bytes: &[u8],
    ) -> Result<Self, DecodeError> {
        With::<_, Self> {
            ctx: DeCtx { typespace, fields },
            _marker: PhantomData,
        }
        .deserialize(spacetimedb_sats::bsatn::de::Deserializer::new(&mut bytes))
    }
}
