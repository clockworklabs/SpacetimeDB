#![deny(unsafe_op_in_unsafe_fn)]

use std::any::TypeId;
use std::collections::{btree_map, BTreeMap};
use std::fmt;
use std::marker::PhantomData;
use std::sync::Mutex;
use std::time::Duration;

use crate::{sys, ReducerContext, ScheduleToken, SpacetimeType, TableType, Timestamp};
use spacetimedb_lib::de::{self, Deserialize, SeqProductAccess};
use spacetimedb_lib::sats::typespace::TypespaceBuilder;
use spacetimedb_lib::sats::{AlgebraicType, AlgebraicTypeRef, ProductTypeElement};
use spacetimedb_lib::ser::{self, Serialize, SerializeSeqProduct};
use spacetimedb_lib::{bsatn, Identity, MiscModuleExport, ModuleDef, ReducerDef, TableDef, TypeAlias};
use sys::Buffer;

pub use once_cell::sync::{Lazy, OnceCell};

scoped_tls::scoped_thread_local! {
    pub(crate) static CURRENT_TIMESTAMP: Timestamp
}

pub fn invoke_reducer<'a, A: Args<'a>, T>(
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

pub fn create_index(index_name: &str, table_id: u32, index_type: sys::raw::IndexType, col_ids: Vec<u8>) -> Buffer {
    let result = sys::create_index(index_name, table_id, index_type as u8, &col_ids);
    cvt_result(result.map_err(cvt_errno))
}

pub fn invoke_connection_func<R: ReducerResult>(
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

    let sender = Identity { data: sender };
    let timestamp = Timestamp::UNIX_EPOCH + Duration::from_micros(timestamp);

    ReducerContext { sender, timestamp }
}

fn cvt_errno(errno: sys::Errno) -> Box<str> {
    let message = format!("{errno}");
    message.into_boxed_str()
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

pub trait ReducerInfo {
    const NAME: &'static str;
    const ARG_NAMES: &'static [Option<&'static str>];
    const INVOKE: ReducerFn;
}

pub trait RepeaterInfo: ReducerInfo {
    const REPEAT_INTERVAL: Duration;
}

pub trait Args<'de>: Sized {
    const LEN: usize;
    fn visit_seq_product<A: SeqProductAccess<'de>>(prod: A) -> Result<Self, A::Error>;
    fn serialize_seq_product<S: SerializeSeqProduct>(&self, prod: &mut S) -> Result<(), S::Error>;
    fn schema<I: ReducerInfo>(typespace: &mut impl TypespaceBuilder) -> ReducerDef;
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
pub const fn assert_table<T: TableType>() {}

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
            fn schema<Info: ReducerInfo>(_typespace: &mut impl TypespaceBuilder) -> ReducerDef {
                #[allow(non_snake_case, irrefutable_let_patterns)]
                let [.., $($T),*] = Info::ARG_NAMES else { panic!() };
                ReducerDef {
                    name: Info::NAME.into(),
                    args: vec![
                        $(ProductTypeElement {
                            name: $T.map(str::to_owned),
                            algebraic_type: <$T>::make_type(_typespace),
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

pub fn schedule<'de, R: ReducerInfo>(time: Timestamp, args: impl ScheduleArgs<'de>) -> ScheduleToken<R> {
    let arg_bytes = bsatn::to_vec(&SerDeArgs(args.into_args())).unwrap();
    let id = sys::schedule(R::NAME, &arg_bytes, time.micros_since_epoch);
    ScheduleToken::new(id)
}

pub fn schedule_repeater<A: RepeaterArgs, T, I: RepeaterInfo>(_reducer: impl for<'de> Reducer<'de, A, T>) {
    let time = schedule_in(I::REPEAT_INTERVAL);
    let args = bsatn::to_vec(&SerDeArgs(A::get_now())).unwrap();
    sys::schedule(I::NAME, &args, time.micros_since_epoch);
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

fn register_describer(f: fn(&mut ModuleBuilder)) {
    DESCRIBERS.lock().unwrap().push(f)
}

pub fn register_reftype<T: SpacetimeType>() {
    register_describer(|module| {
        T::make_type(module);
    })
}

pub fn register_table<T: TableType>() {
    register_describer(|module| {
        let data = *T::make_type(module).as_ref().unwrap();
        let schema = TableDef {
            name: T::TABLE_NAME.into(),
            data,
            column_attrs: T::COLUMN_ATTRS.to_owned(),
            indexes: T::INDEXES.iter().copied().map(Into::into).collect(),
        };
        module.module.tables.push(schema)
    })
}

impl From<crate::IndexDef<'_>> for spacetimedb_lib::IndexDef {
    fn from(index: crate::IndexDef<'_>) -> spacetimedb_lib::IndexDef {
        spacetimedb_lib::IndexDef {
            name: index.name.to_owned(),
            ty: index.ty,
            col_ids: index.col_ids.to_owned(),
        }
    }
}

pub fn register_reducer<'a, A: Args<'a>, T, I: ReducerInfo>(_: impl Reducer<'a, A, T>) {
    register_describer(|module| {
        let schema = A::schema::<I>(module);
        module.module.reducers.push(schema);
        module.reducers.push(I::INVOKE);
    })
}

#[derive(Default)]
struct ModuleBuilder {
    module: ModuleDef,
    reducers: Vec<ReducerFn>,
    type_map: BTreeMap<TypeId, AlgebraicTypeRef>,
}

impl TypespaceBuilder for ModuleBuilder {
    fn add(
        &mut self,
        typeid: TypeId,
        name: Option<&'static str>,
        make_ty: impl FnOnce(&mut Self) -> AlgebraicType,
    ) -> AlgebraicType {
        let r = match self.type_map.entry(typeid) {
            btree_map::Entry::Occupied(o) => *o.get(),
            btree_map::Entry::Vacant(v) => {
                let slot_ref = self.module.typespace.add(AlgebraicType::UNIT_TYPE);
                v.insert(slot_ref);
                if let Some(name) = name {
                    self.module.misc_exports.push(MiscModuleExport::TypeAlias(TypeAlias {
                        name: name.to_owned(),
                        ty: slot_ref,
                    }));
                }
                let ty = make_ty(self);
                self.module.typespace[slot_ref] = ty;
                slot_ref
            }
        };
        AlgebraicType::Ref(r)
    }
}

// not actually a mutex; because wasm is single-threaded this basically just turns into a refcell
static DESCRIBERS: Mutex<Vec<fn(&mut ModuleBuilder)>> = Mutex::new(Vec::new());

pub type ReducerFn = fn(Buffer, u64, &[u8]) -> Buffer;
static REDUCERS: OnceCell<Vec<ReducerFn>> = OnceCell::new();

#[no_mangle]
extern "C" fn __describe_module__() -> Buffer {
    let mut module = ModuleBuilder::default();
    for describer in &*DESCRIBERS.lock().unwrap() {
        describer(&mut module)
    }
    let bytes = bsatn::to_vec(&module.module).expect("unable to serialize typespace");
    REDUCERS.set(module.reducers).ok().unwrap();
    Buffer::alloc(&bytes)
}

#[no_mangle]
extern "C" fn __call_reducer__(id: usize, sender: Buffer, timestamp: u64, args: Buffer) -> Buffer {
    let reducers = REDUCERS.get().unwrap();
    let args = args.read();
    reducers[id](sender, timestamp, &args)
}
