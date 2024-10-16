#![deny(unsafe_op_in_unsafe_fn)]

use crate::timestamp::with_timestamp_set;
use crate::{sys, IterBuf, ReducerContext, ReducerResult, SpacetimeType, Table, Timestamp};
pub use spacetimedb_lib::db::raw_def::v9::Lifecycle as LifecycleReducer;
use spacetimedb_lib::db::raw_def::v9::{RawIndexAlgorithm, RawModuleDefV9Builder, TableType};
use spacetimedb_lib::de::{self, Deserialize, SeqProductAccess};
use spacetimedb_lib::sats::typespace::TypespaceBuilder;
use spacetimedb_lib::sats::{impl_deserialize, impl_serialize, ProductTypeElement};
use spacetimedb_lib::ser::{Serialize, SerializeSeqProduct};
use spacetimedb_lib::{bsatn, Address, Identity, ProductType, RawModuleDef};
use spacetimedb_primitives::*;
use std::fmt;
use std::marker::PhantomData;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use sys::raw::{BytesSink, BytesSource};

/// The `sender` invokes `reducer` at `timestamp` and provides it with the given `args`.
///
/// Returns an invalid buffer on success
/// and otherwise the error is written into the fresh one returned.
pub fn invoke_reducer<'a, A: Args<'a>>(
    reducer: impl Reducer<'a, A>,
    ctx: ReducerContext,
    args: &'a [u8],
) -> Result<(), Box<str>> {
    // Deserialize the arguments from a bsatn encoding.
    let SerDeArgs(args) = bsatn::from_slice(args).expect("unable to decode args");

    // Run the reducer with the environment all set up.
    with_timestamp_set(ctx.timestamp, || reducer.invoke(&ctx, args))
}
/// A trait for types representing the *execution logic* of a reducer.
#[diagnostic::on_unimplemented(
    message = "invalid reducer signature",
    label = "this reducer signature is not valid",
    note = "",
    note = "reducer signatures must match the following pattern:",
    note = "    `Fn(&ReducerContext, [T1, ...]) [-> Result<(), impl Display>]`",
    note = "where each `Ti` type implements `SpacetimeType`.",
    note = ""
)]
pub trait Reducer<'de, A: Args<'de>> {
    fn invoke(&self, ctx: &ReducerContext, args: A) -> ReducerResult;
}

/// A trait for types that can *describe* a reducer.
pub trait ReducerInfo {
    /// The name of the reducer.
    const NAME: &'static str;

    /// The lifecycle of the reducer, if there is one.
    const LIFECYCLE: Option<LifecycleReducer> = None;

    /// A description of the parameter names of the reducer.
    const ARG_NAMES: &'static [Option<&'static str>];

    /// The function to call to invoke the reducer.
    const INVOKE: ReducerFn;
}

/// A trait of types representing the arguments of a reducer.
pub trait Args<'de>: Sized {
    /// How many arguments does the reducer accept?
    const LEN: usize;

    /// Deserialize the arguments from the sequence `prod` which knows when there are next elements.
    fn visit_seq_product<A: SeqProductAccess<'de>>(prod: A) -> Result<Self, A::Error>;

    /// Serialize the arguments in `self` into the sequence `prod` according to the type `S`.
    fn serialize_seq_product<S: SerializeSeqProduct>(&self, prod: &mut S) -> Result<(), S::Error>;

    /// Returns the schema for this reducer provided a `typespace`.
    fn schema<I: ReducerInfo>(typespace: &mut impl TypespaceBuilder) -> ProductType;
}

/// A trait of types representing the result of executing a reducer.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a valid reducer return type",
    note = "reducers cannot return values -- you can only return `()` or `Result<(), impl Display>`"
)]
pub trait IntoReducerResult {
    /// Convert the result into form where there is no value
    /// and the error message is a string.
    fn into_result(self) -> Result<(), Box<str>>;
}
impl IntoReducerResult for () {
    #[inline]
    fn into_result(self) -> Result<(), Box<str>> {
        Ok(self)
    }
}
impl<E: fmt::Display> IntoReducerResult for Result<(), E> {
    #[inline]
    fn into_result(self) -> Result<(), Box<str>> {
        self.map_err(|e| e.to_string().into())
    }
}

#[diagnostic::on_unimplemented(
    message = "the first argument of a reducer must be `&ReducerContext`",
    note = "all reducers must take `&ReducerContext` as their first argument"
)]
pub trait ReducerContextArg {
    // a little hack used in the macro to make error messages nicer. it generates <T as ReducerContextArg>::_ITEM
    #[doc(hidden)]
    const _ITEM: () = ();
}
impl ReducerContextArg for &ReducerContext {}

/// A trait of types that can be an argument of a reducer.
#[diagnostic::on_unimplemented(
    message = "the reducer argument `{Self}` does not implement `SpacetimeType`",
    note = "if you own the type, try adding `#[derive(SpacetimeType)]` to its definition"
)]
pub trait ReducerArg {
    // a little hack used in the macro to make error messages nicer. it generates <T as ReducerArg>::_ITEM
    #[doc(hidden)]
    const _ITEM: () = ();
}
impl<T: SpacetimeType> ReducerArg for T {}

/// Assert that a reducer type-checks with a given type.
pub const fn scheduled_reducer_typecheck<'de, Row>(_x: impl ReducerForScheduledTable<'de, Row>)
where
    Row: SpacetimeType + Serialize + Deserialize<'de>,
{
    core::mem::forget(_x);
}

#[diagnostic::on_unimplemented(
    message = "invalid signature for scheduled table reducer",
    note = "the scheduled reducer must take `{TableRow}` as its sole argument",
    note = "e.g: `fn scheduled_reducer(ctx: &ReducerContext, arg: {TableRow})`"
)]
pub trait ReducerForScheduledTable<'de, TableRow> {}
impl<'de, TableRow: SpacetimeType + Serialize + Deserialize<'de>, R: Reducer<'de, (TableRow,)>>
    ReducerForScheduledTable<'de, TableRow> for R
{
}

// the macro generates <T as SpacetimeType>::make_type::<DummyTypespace>
pub struct DummyTypespace;
impl TypespaceBuilder for DummyTypespace {
    fn add(
        &mut self,
        _: std::any::TypeId,
        _: Option<&'static str>,
        _: impl FnOnce(&mut Self) -> spacetimedb_lib::AlgebraicType,
    ) -> spacetimedb_lib::AlgebraicType {
        unreachable!()
    }
}

#[diagnostic::on_unimplemented(
    message = "the column type `{Self}` does not implement `SpacetimeType`",
    note = "table column types all must implement `SpacetimeType`",
    note = "if you own the type, try adding `#[derive(SpacetimeType)]` to its definition"
)]
pub trait TableColumn {
    // a little hack used in the macro to make error messages nicer. it generates <T as TableColumn>::_ITEM
    #[doc(hidden)]
    const _ITEM: () = ();
}
impl<T: SpacetimeType> TableColumn for T {}

/// Used in the last type parameter of `Reducer` to indicate that the
/// context argument *should* be passed to the reducer logic.
pub struct ContextArg;

/// A visitor providing a deserializer for a type `A: Args`.
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
        // Implement `Args` for the tuple type `($($T,)*)`.
        impl<'de, $($T: SpacetimeType + Deserialize<'de> + Serialize),*> Args<'de> for ($($T,)*) {
            const LEN: usize = impl_reducer!(@count $($T)*);
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn visit_seq_product<Acc: SeqProductAccess<'de>>(mut prod: Acc) -> Result<Self, Acc::Error> {
                let vis = ArgsVisitor { _marker: PhantomData::<Self> };
                // Counts the field number; only relevant for errors.
                let i = 0;
                // For every element in the product, deserialize.
                $(
                    let $T = prod.next_element::<$T>()?.ok_or_else(|| de::Error::missing_field(i, None, &vis))?;
                    let i = i + 1;
                )*
                Ok(($($T,)*))
            }

            #[allow(non_snake_case)]
            fn serialize_seq_product<Ser: SerializeSeqProduct>(&self, _prod: &mut Ser) -> Result<(), Ser::Error> {
                // For every element in the product, serialize.
                let ($($T,)*) = self;
                $(_prod.serialize_element($T)?;)*
                Ok(())
            }

            #[inline]
            #[allow(non_snake_case, irrefutable_let_patterns)]
            fn schema<Info: ReducerInfo>(_typespace: &mut impl TypespaceBuilder) -> ProductType {
                // Extract the names of the arguments.
                let [.., $($T),*] = Info::ARG_NAMES else { panic!() };
                ProductType::new(vec![
                        $(ProductTypeElement {
                            name: $T.map(Into::into),
                            algebraic_type: <$T>::make_type(_typespace),
                        }),*
                ].into())
            }
        }

        // Implement `Reducer<..., ContextArg>` for the tuple type `($($T,)*)`.
        impl<'de, Func, Ret, $($T: SpacetimeType + Deserialize<'de> + Serialize),*> Reducer<'de, ($($T,)*)> for Func
        where
            Func: Fn(&ReducerContext, $($T),*) -> Ret,
            Ret: IntoReducerResult
        {
            #[allow(non_snake_case)]
            fn invoke(&self, ctx: &ReducerContext, args: ($($T,)*)) -> Result<(), Box<str>> {
                let ($($T,)*) = args;
                self(ctx, $($T),*).into_result()
            }
        }

    };
    // Counts the number of elements in the tuple.
    (@count $($T:ident)*) => {
        0 $(+ impl_reducer!(@drop $T 1))*
    };
    (@drop $a:tt $b:tt) => { $b };
}

impl_reducer!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z, AA, AB, AC, AD, AE, AF);

/// Provides deserialization and serialization for any type `A: Args`.
struct SerDeArgs<A>(A);
impl_deserialize!(
    [A: Args<'de>] SerDeArgs<A>,
    de => de.deserialize_product(ArgsVisitor { _marker: PhantomData }).map(Self)
);
impl_serialize!(['de, A: Args<'de>] SerDeArgs<A>, (self, ser) => {
    let mut prod = ser.serialize_seq_product(A::LEN)?;
    self.0.serialize_seq_product(&mut prod)?;
    prod.end()
});

/// A trait for types representing repeater arguments.
pub trait RepeaterArgs: for<'de> Args<'de> {
    /// Returns a notion of now in time.
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

/// Registers into `DESCRIBERS` a function `f` to modify the module builder.
fn register_describer(f: fn(&mut ModuleBuilder)) {
    DESCRIBERS.lock().unwrap().push(f)
}

/// Registers a describer for the `SpacetimeType` `T`.
pub fn register_reftype<T: SpacetimeType>() {
    register_describer(|module| {
        T::make_type(&mut module.inner);
    })
}

/// Registers a describer for the `TableType` `T`.
pub fn register_table<T: Table>() {
    register_describer(|module| {
        let product_type_ref = *T::Row::make_type(&mut module.inner).as_ref().unwrap();

        let mut table = module
            .inner
            .build_table(T::TABLE_NAME, product_type_ref)
            .with_type(TableType::User)
            .with_access(T::TABLE_ACCESS);

        for &col in T::UNIQUE_COLUMNS {
            table = table.with_unique_constraint(col, None);
        }
        for &index in T::INDEXES {
            table = table.with_index(index.algo.into(), index.accessor_name, Some(index.name.into()));
        }
        if let Some(primary_key) = T::PRIMARY_KEY {
            table = table.with_primary_key(primary_key);
        }
        for &col in T::SEQUENCES {
            table = table.with_column_sequence(col, None);
        }
        if let Some(scheduled_reducer) = T::SCHEDULED_REDUCER_NAME {
            table = table.with_schedule(scheduled_reducer, None);
        }

        table.finish();
    })
}

impl From<crate::table::IndexAlgo<'_>> for RawIndexAlgorithm {
    fn from(algo: crate::table::IndexAlgo<'_>) -> RawIndexAlgorithm {
        match algo {
            crate::table::IndexAlgo::BTree { columns } => RawIndexAlgorithm::BTree {
                columns: columns.iter().copied().collect(),
            },
        }
    }
}

/// Registers a describer for the reducer `I` with arguments `A`.
pub fn register_reducer<'a, A: Args<'a>, I: ReducerInfo>(_: impl Reducer<'a, A>) {
    register_describer(|module| {
        let params = A::schema::<I>(&mut module.inner);
        module.inner.add_reducer(I::NAME, params, I::LIFECYCLE);
        module.reducers.push(I::INVOKE);
    })
}

/// A builder for a module.
#[derive(Default)]
struct ModuleBuilder {
    /// The module definition.
    inner: RawModuleDefV9Builder,
    /// The reducers of the module.
    reducers: Vec<ReducerFn>,
}

// Not actually a mutex; because WASM is single-threaded this basically just turns into a refcell.
static DESCRIBERS: Mutex<Vec<fn(&mut ModuleBuilder)>> = Mutex::new(Vec::new());

/// A reducer function takes in `(Sender, Timestamp, Args)`
/// and returns a result with a possible error message.
pub type ReducerFn = fn(ReducerContext, &[u8]) -> ReducerResult;
static REDUCERS: OnceLock<Vec<ReducerFn>> = OnceLock::new();

/// Called by the host when the module is initialized
/// to describe the module into a serialized form that is returned.
///
/// This is also the module's opportunity to ready `__call_reducer__`
/// (by writing the set of `REDUCERS`).
///
/// To `description`, a BSATN-encoded ModuleDef` should be written,.
/// For the time being, the definition of `ModuleDef` is not stabilized,
/// as it is being changed by the schema proposal.
///
/// The `ModuleDef` is used to define tables, constraints, indices, reducers, etc.
/// This affords the module the opportunity
/// to define and, to a limited extent, alter the schema at initialization time,
/// including when modules are updated (re-publishing).
/// After initialization, the module cannot alter the schema.
#[no_mangle]
extern "C" fn __describe_module__(description: BytesSink) {
    // Collect the `module`.
    let mut module = ModuleBuilder::default();
    for describer in &*DESCRIBERS.lock().unwrap() {
        describer(&mut module)
    }

    // Serialize the module to bsatn.
    let module_def = module.inner.finish();
    let module_def = RawModuleDef::V9(module_def);
    let bytes = bsatn::to_vec(&module_def).expect("unable to serialize typespace");

    // Write the set of reducers.
    REDUCERS.set(module.reducers).ok().unwrap();

    // Write the bsatn data into the sink.
    write_to_sink(description, &bytes);
}

// TODO(1.0): update `__call_reducer__` docs + for `BytesSink`.

/// Called by the host to execute a reducer
/// when the `sender` calls the reducer identified by `id` at `timestamp` with `args`.
///
/// The `sender_{0-3}` are the pieces of a `[u8; 32]` (`u256`) representing the sender's `Identity`.
/// They are encoded as follows (assuming `identity.identity_bytes: [u8; 32]`):
/// - `sender_0` contains bytes `[0 ..8 ]`.
/// - `sender_1` contains bytes `[8 ..16]`.
/// - `sender_2` contains bytes `[16..24]`.
/// - `sender_3` contains bytes `[24..32]`.
///
/// The `address_{0-1}` are the pieces of a `[u8; 16]` (`u128`) representing the callers's `Address`.
/// They are encoded as follows (assuming `identity.__address_bytes: [u8; 16]`):
/// - `address_0` contains bytes `[0 ..8 ]`.
/// - `address_1` contains bytes `[8 ..16]`.
///
/// The `args` is a `BytesSource`, registered on the host side,
/// which can be read with `bytes_source_read`.
/// The contents of the buffer are the BSATN-encoding of the arguments to the reducer.
/// In the case of empty arguments, `args` will be 0, that is, invalid.
///
/// The `error` is a `BytesSink`, registered on the host side,
/// which can be written to with `bytes_sink_write`.
/// When `error` is written to,
/// it is expected that `HOST_CALL_FAILURE` is returned.
/// Otherwise, `0` should be returned, i.e., the reducer completed successfully.
/// Note that in the future, more failure codes could be supported.
#[no_mangle]
extern "C" fn __call_reducer__(
    id: usize,
    sender_0: u64,
    sender_1: u64,
    sender_2: u64,
    sender_3: u64,
    address_0: u64,
    address_1: u64,
    timestamp: u64,
    args: BytesSource,
    error: BytesSink,
) -> i16 {
    // Piece together `sender_i` into an `Identity`.
    let sender = [sender_0, sender_1, sender_2, sender_3];
    let sender: [u8; 32] = bytemuck::must_cast(sender);
    let sender = Identity::from_byte_array(sender);

    // Piece together `address_i` into an `Address`.
    // The all-zeros `address` (`Address::__DUMMY`) is interpreted as `None`.
    let address = [address_0, address_1];
    let address: [u8; 16] = bytemuck::must_cast(address);
    let address = Address::from_byte_array(address);
    let address = (address != Address::__DUMMY).then_some(address);

    // Assemble the `ReducerContext`.
    let timestamp = Timestamp::UNIX_EPOCH + Duration::from_micros(timestamp);
    let ctx = ReducerContext {
        db: crate::Local {},
        sender,
        timestamp,
        address,
        rng: std::cell::OnceCell::new(),
    };

    // Fetch reducer function.
    let reducers = REDUCERS.get().unwrap();
    // Dispatch to it with the arguments read.
    let res = with_read_args(args, |args| reducers[id](ctx, args));
    // Convert any error message to an error code and writes to the `error` sink.
    match res {
        Ok(()) => 0,
        Err(msg) => {
            write_to_sink(error, msg.as_bytes());
            errno::HOST_CALL_FAILURE.get() as i16
        }
    }
}

/// Run `logic` with `args` read from the host into a `&[u8]`.
fn with_read_args<R>(args: BytesSource, logic: impl FnOnce(&[u8]) -> R) -> R {
    if args == BytesSource::INVALID {
        return logic(&[]);
    }

    // Steal an iteration row buffer.
    // These were not meant for this purpose,
    // but it's likely we have one sitting around being unused at this point,
    // so use it to avoid allocating a temporary buffer if possible.
    // And if we do allocate a temporary buffer now, it will likely be reused later.
    let mut buf = IterBuf::take();

    // Read `args` and run `logic`.
    read_bytes_source_into(args, &mut buf);
    logic(&buf)
}

const NO_SPACE: u16 = errno::NO_SPACE.get();
const NO_SUCH_BYTES: u16 = errno::NO_SUCH_BYTES.get();

/// Read `source` from the host fully into `buf`.
fn read_bytes_source_into(source: BytesSource, buf: &mut Vec<u8>) {
    const INVALID: i16 = NO_SUCH_BYTES as i16;

    loop {
        // Write into the spare capacity of the buffer.
        let buf_ptr = buf.spare_capacity_mut();
        let spare_len = buf_ptr.len();
        let mut buf_len = buf_ptr.len();
        let buf_ptr = buf_ptr.as_mut_ptr().cast();
        let ret = unsafe { sys::raw::bytes_source_read(source, buf_ptr, &mut buf_len) };
        if ret <= 0 {
            // SAFETY: `bytes_source_read` just appended `spare_len` bytes to `buf`.
            unsafe { buf.set_len(buf.len() + spare_len) };
        }
        match ret {
            // Host side source exhausted, we're done.
            -1 => break,
            // Wrote the entire spare capacity.
            // Need to reserve more space in the buffer.
            0 if spare_len == buf_len => buf.reserve(1024),
            // Host didn't write as much as possible.
            // Try to read some more.
            // The host will likely not trigger this branch (current host doesn't),
            // but a module should be prepared for it.
            0 => {}
            INVALID => panic!("invalid source passed"),
            _ => unreachable!(),
        }
    }
}

/// Write `buf` to `sink`.
fn write_to_sink(sink: BytesSink, mut buf: &[u8]) {
    loop {
        let len = &mut buf.len();
        match unsafe { sys::raw::bytes_sink_write(sink, buf.as_ptr(), len) } {
            0 => {
                // Set `buf` to remainder and bail if it's empty.
                (_, buf) = buf.split_at(*len);
                if buf.is_empty() {
                    break;
                }
            }
            NO_SUCH_BYTES => panic!("invalid sink passed"),
            NO_SPACE => panic!("no space left at sink"),
            _ => unreachable!(),
        }
    }
}

#[macro_export]
#[doc(hidden)]
macro_rules! __make_register_reftype {
    ($ty:ty, $name:literal) => {
        const _: () = {
            #[export_name = concat!("__preinit__20_register_describer_", $name)]
            extern "C" fn __register_describer() {
                $crate::rt::register_reftype::<$ty>()
            }
        };
    };
}

#[cfg(feature = "unstable")]
#[doc(hidden)]
pub fn volatile_nonatomic_schedule_immediate<'de, A: Args<'de>, R: Reducer<'de, A>, R2: ReducerInfo>(
    _reducer: R,
    args: A,
) {
    let arg_bytes = bsatn::to_vec(&SerDeArgs(args)).unwrap();

    // Schedule the reducer.
    sys::volatile_nonatomic_schedule_immediate(R2::NAME, &arg_bytes)
}
