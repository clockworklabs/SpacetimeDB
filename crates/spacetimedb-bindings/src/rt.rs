#![deny(unsafe_op_in_unsafe_fn)]

use std::fmt;
use std::time::Duration;

use crate::{sys, FromValue, IntoValue, ReducerContext, Timestamp};
use spacetimedb_lib::{ElementDef, Hash, ReducerDef, TupleValue};
use sys::Buffer;

pub use once_cell::sync::{Lazy, OnceCell};

scoped_tls::scoped_thread_local! {
    pub(crate) static CURRENT_TIMESTAMP: Timestamp
}

pub unsafe fn invoke_reducer<A: Args, T>(
    reducer: impl Reducer<A, T>,
    schema: &ReducerDef,
    sender: Buffer,
    timestamp: u64,
    args: Buffer,
    epilogue: impl FnOnce(Result<(), &str>),
) -> Buffer {
    let ctx = assemble_context(sender, timestamp);
    let args = args.read();

    let mut rdr = &args[..];
    let args = TupleValue::decode_from_elements(&schema.args, &mut rdr)
        .ok()
        .and_then(A::from_tuple)
        .expect("unable to decode args");

    let res = CURRENT_TIMESTAMP.set(&{ ctx.timestamp }, || {
        let res = reducer.invoke(ctx, args);
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
        Ok(()) => sys::raw::INVALID_BUFFER,
        Err(errmsg) => Buffer::alloc(errmsg.as_bytes()),
    }
}

pub trait Reducer<A: Args, T> {
    fn invoke(&self, ctx: ReducerContext, args: A) -> Result<(), Box<str>>;
}

pub trait Args: Sized {
    fn from_tuple(tup: TupleValue) -> Option<Self>;
    fn into_tuple(self) -> TupleValue;
    fn schema(name: &str, arg_names: &[Option<&str>]) -> ReducerDef;
}

pub trait ScheduleArgs: Sized {
    type Args: Args;
    fn into_args(self) -> Self::Args;
}
impl<T: Args> ScheduleArgs for T {
    type Args = Self;
    fn into_args(self) -> Self::Args {
        self
    }
}

pub fn schema_of_func<A: Args, T>(_: impl Reducer<A, T>, name: &str, arg_names: &[Option<&str>]) -> ReducerDef {
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

pub trait ReducerArg {}
impl<T: FromValue> ReducerArg for T {}
impl ReducerArg for ReducerContext {}
pub fn assert_reducerarg<T: ReducerArg>() {}
pub fn assert_reducerret<T: ReducerResult>() {}

pub struct ContextArg;
pub struct NoContextArg;

macro_rules! impl_reducer {
    ($($T1:ident $(, $T:ident)*)?) => {
        impl_reducer!(@impl $($T1 $(, $T)*)?);
        $(impl_reducer!($($T),*);)?
    };
    (@impl $($T:ident),*) => {
        impl<$($T: FromValue + IntoValue),*> Args for ($($T,)*) {
            fn from_tuple(tup: TupleValue) -> Option<Self> {
                let tup: Box<[_; impl_reducer!(@count $($T)*)]> = tup.elements.try_into().ok()?;
                #[allow(non_snake_case)]
                let [$($T),*] = *tup;
                Some(($(FromValue::from_value($T)?,)*))
            }
            fn into_tuple(self) -> TupleValue {
                #[allow(non_snake_case)]
                let ($($T,)*) = self;
                TupleValue { elements: Box::new([$(IntoValue::into_value($T),)*]) }
            }
            #[inline]
            fn schema(name: &str, arg_names: &[Option<&str>]) -> ReducerDef {
                #[allow(non_snake_case, irrefutable_let_patterns)]
                let [.., $($T),*] = arg_names else { panic!() };
                let mut _n = 0;
                ReducerDef {
                    name: Some(name.into()),
                    args: vec![
                        $(ElementDef {
                            tag: { let n = _n; _n += 1; n },
                            name: $T.map(str::to_owned),
                            element_type: <$T>::get_schema(),
                        }),*
                    ],
                }
            }
        }
        impl<$($T: FromValue + IntoValue),*> ScheduleArgs for (ReducerContext, $($T,)*) {
            type Args = ($($T,)*);
            fn into_args(self) -> Self::Args {
                #[allow(non_snake_case)]
                let (_ctx, $($T,)*) = self;
                ($($T,)*)
            }
        }
        impl<Func, Ret, $($T: FromValue + IntoValue),*> Reducer<($($T,)*), ContextArg> for Func
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
        impl<Func, Ret, $($T: FromValue + IntoValue),*> Reducer<($($T,)*), NoContextArg> for Func
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

#[track_caller]
pub fn schedule_in(dur: Duration) -> Timestamp {
    Timestamp::now()
        .checked_add(dur)
        .unwrap_or_else(|| panic!("{dur:?} is too far into the future to schedule"))
}

pub fn schedule(reducer_name: &str, time: Timestamp, args: impl ScheduleArgs) {
    let mut arg_bytes = Vec::new();
    args.into_args().into_tuple().encode(&mut arg_bytes);
    sys::schedule(reducer_name, &arg_bytes, time.micros_since_epoch)
}

pub fn schedule_repeater<A: RepeaterArgs, T>(_reducer: impl Reducer<A, T>, name: &str, dur: Duration) {
    let time = schedule_in(dur);
    let mut args = Vec::new();
    A::KIND.to_tuple().encode(&mut args);
    sys::schedule(name, &args, time.micros_since_epoch)
}

pub enum RepeaterArgsKind {
    Empty,
    TimestampArg,
}

impl RepeaterArgsKind {
    #[inline]
    fn to_tuple(self) -> TupleValue {
        let elements: Box<[_]> = match self {
            RepeaterArgsKind::Empty => Box::new([]),
            RepeaterArgsKind::TimestampArg => Box::new([Timestamp::now().into_value()]),
        };
        TupleValue { elements }
    }
}

pub trait RepeaterArgs: Args {
    const KIND: RepeaterArgsKind;
}

impl RepeaterArgs for () {
    const KIND: RepeaterArgsKind = RepeaterArgsKind::Empty;
}

impl RepeaterArgs for (Timestamp,) {
    const KIND: RepeaterArgsKind = RepeaterArgsKind::TimestampArg;
}
