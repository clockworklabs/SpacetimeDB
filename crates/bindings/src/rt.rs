#![deny(unsafe_op_in_unsafe_fn)]

use std::fmt;
use std::marker::PhantomData;
use std::sync::Mutex;
use std::time::Duration;

use crate::{sys, ReducerContext, RefType, SpacetimeType, TableType, Timestamp};
use spacetimedb_lib::de::{self, Deserialize, SeqProductAccess};
use spacetimedb_lib::sats::{AlgebraicType, AlgebraicTypeRef, Typespace};
use spacetimedb_lib::ser::{self, Serialize, SerializeSeqProduct};
use spacetimedb_lib::{bsatn, ElementDef, Hash, ReducerDef};
use sys::Buffer;

pub use log;
pub use once_cell::sync::{Lazy, OnceCell};

scoped_tls::scoped_thread_local! {
    pub(crate) static CURRENT_TIMESTAMP: Timestamp
}

pub unsafe fn invoke_reducer<'a, A: Args<'a>, T>(
    reducer: impl Reducer<'a, A, T>,
    sender: Buffer,
    timestamp: u64,
    args: &'a [u8],
    epilogue: impl FnOnce(Result<(), &str>),
) -> Buffer {
    let ctx = assemble_context(sender, timestamp);

    let SerDeArgs(args) = bsatn::from_slice(args).expect("unable to decode args");

    let res = CURRENT_TIMESTAMP.set(&{ ctx.timestamp }, || {
        let res: Result<(), Box<str>> = reducer.invoke(ctx, args);
        epilogue(res.as_ref().map(|()| ()).map_err(|e| &**e));
        res
    });
    cvt_result(res)
}

pub unsafe fn invoke_connection_func<R: ReducerResult>(
    f: impl Fn(ReducerContext) -> R,
    sender: Buffer,
    timestamp: u64,
) -> Buffer {
    let ctx = assemble_context(sender, timestamp);

    let res = CURRENT_TIMESTAMP.set(&{ ctx.timestamp }, || f(ctx).into_result());
    cvt_result(res)
}

fn assemble_context(sender: Buffer, timestamp: u64) -> ReducerContext {
    let sender = sender.read_array::<32>();

    let sender = Hash { data: sender };
    let timestamp = Timestamp::UNIX_EPOCH + Duration::from_micros(timestamp);

    ReducerContext { sender, timestamp }
}

fn cvt_result(res: Result<(), Box<str>>) -> Buffer {
    match res {
        Ok(()) => Buffer::INVALID,
        Err(errmsg) => Buffer::alloc(errmsg.as_bytes()),
    }
}

pub trait Reducer<'de, A: Args<'de>, T> {
    fn invoke(&self, ctx: ReducerContext, args: A) -> Result<(), Box<str>>;
}

pub trait Args<'de>: Sized {
    const LEN: usize;
    fn visit_seq_product<A: SeqProductAccess<'de>>(prod: A) -> Result<Self, A::Error>;
    fn serialize_seq_product<S: SerializeSeqProduct>(&self, prod: &mut S) -> Result<(), S::Error>;
    fn schema(name: &str, arg_names: &[Option<&str>]) -> ReducerDef;
}

pub trait ScheduleArgs<'de>: Sized {
    type Args: Args<'de>;
    fn into_args(self) -> Self::Args;
}
impl<'de, T: Args<'de>> ScheduleArgs<'de> for T {
    type Args = Self;
    fn into_args(self) -> Self::Args {
        self
    }
}

pub fn schema_of_func<'a, A: Args<'a>, T>(
    _: impl Reducer<'a, A, T>,
    name: &str,
    arg_names: &[Option<&str>],
) -> ReducerDef {
    A::schema(name, arg_names)
}

pub trait ReducerResult {
    fn into_result(self) -> Result<(), Box<str>>;
}
impl ReducerResult for () {
    #[inline]
    fn into_result(self) -> Result<(), Box<str>> {
        Ok(self)
    }
}
impl<E: fmt::Debug> ReducerResult for Result<(), E> {
    #[inline]
    fn into_result(self) -> Result<(), Box<str>> {
        self.map_err(|e| format!("{e:?}").into())
    }
}

pub trait ReducerArg<'de> {}
impl<'de, T: Deserialize<'de>> ReducerArg<'de> for T {}
impl<'de> ReducerArg<'de> for ReducerContext {}
pub fn assert_reducerarg<'de, T: ReducerArg<'de>>() {}
pub fn assert_reducerret<T: ReducerResult>() {}

pub struct ContextArg;
pub struct NoContextArg;

struct ArgsVisitor<A> {
    _marker: PhantomData<A>,
}

impl<'de, A: Args<'de>> de::ProductVisitor<'de> for ArgsVisitor<A> {
    type Output = A;

    fn product_name(&self) -> Option<&str> {
        None
    }
    fn product_len(&self) -> usize {
        A::LEN
    }
    fn product_kind(&self) -> de::ProductKind {
        de::ProductKind::ReducerArgs
    }
    fn visit_seq_product<Acc: SeqProductAccess<'de>>(self, prod: Acc) -> Result<Self::Output, Acc::Error> {
        A::visit_seq_product(prod)
    }
    fn visit_named_product<Acc: de::NamedProductAccess<'de>>(self, _prod: Acc) -> Result<Self::Output, Acc::Error> {
        Err(de::Error::custom("named products not supported"))
    }
}

macro_rules! impl_reducer {
    ($($T1:ident $(, $T:ident)*)?) => {
        impl_reducer!(@impl $($T1 $(, $T)*)?);
        $(impl_reducer!($($T),*);)?
    };
    (@impl $($T:ident),*) => {
        impl<'de, $($T: SpacetimeType + Deserialize<'de> + Serialize),*> Args<'de> for ($($T,)*) {
            const LEN: usize = impl_reducer!(@count $($T)*);
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn visit_seq_product<Acc: SeqProductAccess<'de>>(mut prod: Acc) -> Result<Self, Acc::Error> {
                let vis = ArgsVisitor { _marker: PhantomData::<Self> };
                let i = 0;
                $(let $T = prod.next_element::<$T>()?.ok_or_else(|| de::Error::missing_field(i, None, &vis))?;
                  let i = i + 1;)*
                Ok(($($T,)*))
            }
            fn serialize_seq_product<Ser: SerializeSeqProduct>(&self, _prod: &mut Ser) -> Result<(), Ser::Error> {
                #[allow(non_snake_case)]
                let ($($T,)*) = self;
                $(_prod.serialize_element($T)?;)*
                Ok(())
            }
            #[inline]
            fn schema(name: &str, arg_names: &[Option<&str>]) -> ReducerDef {
                #[allow(non_snake_case, irrefutable_let_patterns)]
                let [.., $($T),*] = arg_names else { panic!() };
                ReducerDef {
                    name: Some(name.into()),
                    args: vec![
                        $(ElementDef {
                            name: $T.map(str::to_owned),
                            algebraic_type: <$T>::get_schema(),
                        }),*
                    ],
                }
            }
        }
        impl<'de, $($T: SpacetimeType + Deserialize<'de> + Serialize),*> ScheduleArgs<'de> for (ReducerContext, $($T,)*) {
            type Args = ($($T,)*);
            #[allow(clippy::unused_unit)]
            fn into_args(self) -> Self::Args {
                #[allow(non_snake_case)]
                let (_ctx, $($T,)*) = self;
                ($($T,)*)
            }
        }
        impl<'de, Func, Ret, $($T: SpacetimeType + Deserialize<'de> + Serialize),*> Reducer<'de, ($($T,)*), ContextArg> for Func
        where
            Func: Fn(ReducerContext, $($T),*) -> Ret,
            Ret: ReducerResult
        {
            fn invoke(&self, ctx: ReducerContext, args: ($($T,)*)) -> Result<(), Box<str>> {
                #[allow(non_snake_case)]
                let ($($T,)*) = args;
                self(ctx, $($T),*).into_result()
            }
        }
        impl<'de, Func, Ret, $($T: SpacetimeType + Deserialize<'de> + Serialize),*> Reducer<'de, ($($T,)*), NoContextArg> for Func
        where
            Func: Fn($($T),*) -> Ret,
            Ret: ReducerResult
        {
            fn invoke(&self, _ctx: ReducerContext, args: ($($T,)*)) -> Result<(), Box<str>> {
                #[allow(non_snake_case)]
                let ($($T,)*) = args;
                self($($T),*).into_result()
            }
        }
    };
    (@count $($T:ident)*) => {
        0 $(+ impl_reducer!(@drop $T 1))*
    };
    (@drop $a:tt $b:tt) => { $b };
}

impl_reducer!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z, AA, AB, AC, AD, AE, AF);

struct SerDeArgs<A>(A);
impl<'de, A: Args<'de>> Deserialize<'de> for SerDeArgs<A> {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer
            .deserialize_product(ArgsVisitor { _marker: PhantomData })
            .map(Self)
    }
}
impl<'de, A: Args<'de>> Serialize for SerDeArgs<A> {
    fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut prod = serializer.serialize_seq_product(A::LEN)?;
        self.0.serialize_seq_product(&mut prod)?;
        prod.end()
    }
}

#[track_caller]
pub fn schedule_in(dur: Duration) -> Timestamp {
    Timestamp::now()
        .checked_add(dur)
        .unwrap_or_else(|| panic!("{dur:?} is too far into the future to schedule"))
}

pub fn schedule<'de>(reducer_name: &str, time: Timestamp, args: impl ScheduleArgs<'de>) {
    let arg_bytes = bsatn::to_vec(&SerDeArgs(args.into_args())).unwrap();
    sys::schedule(reducer_name, &arg_bytes, time.micros_since_epoch)
}

pub fn schedule_repeater<A: RepeaterArgs, T>(_reducer: impl for<'de> Reducer<'de, A, T>, name: &str, dur: Duration) {
    let time = schedule_in(dur);
    let args = bsatn::to_vec(&SerDeArgs(A::get_now())).unwrap();
    sys::schedule(name, &args, time.micros_since_epoch)
}

pub trait RepeaterArgs: for<'de> Args<'de> {
    fn get_now() -> Self;
}

impl RepeaterArgs for () {
    fn get_now() -> Self {}
}

impl RepeaterArgs for (Timestamp,) {
    fn get_now() -> Self {
        (Timestamp::now(),)
    }
}

pub fn describe_reftype<T: RefType>() -> u32 {
    T::typeref().0
}

pub fn describe_table<T: TableType>() -> Buffer {
    Buffer::alloc(&bsatn::to_vec(&T::get_tabledef()).unwrap())
}

// not actually a mutex; because wasm is single-threaded this basically just turns into a refcell
static TYPESPACE_BUILDER: Mutex<Typespace> = Mutex::new(Typespace::new(Vec::new()));

pub fn alloc_typespace_slot() -> AlgebraicTypeRef {
    TYPESPACE_BUILDER.lock().unwrap().add(AlgebraicType::UNIT_TYPE)
}

pub fn set_typespace_slot(slot: AlgebraicTypeRef, ty: AlgebraicType) {
    TYPESPACE_BUILDER.lock().unwrap()[slot] = ty
}

fn finalize_typespace() {
    let typespace = std::mem::take(&mut *TYPESPACE_BUILDER.lock().unwrap());
    GLOBAL_TYPESPACE.inner.set(typespace).unwrap();
}

pub struct GlobalTypespace {
    inner: OnceCell<Typespace>,
}

impl std::ops::Deref for GlobalTypespace {
    type Target = Typespace;
    fn deref(&self) -> &Self::Target {
        self.inner.get().unwrap()
    }
}

pub static GLOBAL_TYPESPACE: GlobalTypespace = GlobalTypespace { inner: OnceCell::new() };

#[no_mangle]
extern "C" fn __preinit__30_finalize_typespace() {
    finalize_typespace()
}

#[no_mangle]
extern "C" fn __describe_typespace__() -> Buffer {
    let bytes = bsatn::to_vec(&*GLOBAL_TYPESPACE).expect("unable to serialize typespace");
    Buffer::alloc(&bytes)
}
