#![deny(unsafe_op_in_unsafe_fn)]

use crate::timestamp::with_timestamp_set;
use crate::{return_iter_buf, sys, take_iter_buf, ReducerContext, ReducerResult, SpacetimeType, TableType, Timestamp};
use spacetimedb_lib::db::auth::StTableType;
use spacetimedb_lib::db::raw_def::{
    RawColumnDefV8, RawConstraintDefV8, RawIndexDefV8, RawSequenceDefV8, RawTableDefV8,
};
use spacetimedb_lib::de::{self, Deserialize, SeqProductAccess};
use spacetimedb_lib::sats::typespace::TypespaceBuilder;
use spacetimedb_lib::sats::{impl_deserialize, impl_serialize, ProductTypeElement};
use spacetimedb_lib::ser::{Serialize, SerializeSeqProduct};
use spacetimedb_lib::{bsatn, Address, Identity, ModuleDefBuilder, RawModuleDef, ReducerDef, TableDesc};
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
pub fn invoke_reducer<'a, A: Args<'a>, T>(
    reducer: impl Reducer<'a, A, T>,
    ctx: ReducerContext,
    args: &'a [u8],
) -> Result<(), Box<str>> {
    // Deserialize the arguments from a bsatn encoding.
    let SerDeArgs(args) = bsatn::from_slice(args).expect("unable to decode args");

    // Run the reducer with the environment all set up.
    let invoke = || reducer.invoke(ctx, args);
    #[cfg(feature = "rand")]
    let invoke = || crate::rng::with_rng_set(invoke);
    with_timestamp_set(ctx.timestamp, invoke)
}
/// A trait for types representing the *execution logic* of a reducer.
///
/// The type parameter `T` is used for determining whether there is a context argument.
pub trait Reducer<'de, A: Args<'de>, T> {
    fn invoke(&self, ctx: ReducerContext, args: A) -> ReducerResult;

    type ArgsWithContext;
    fn extract_args(args: Self::ArgsWithContext) -> A;
}

/// A trait for types that can *describe* a reducer.
pub trait ReducerInfo {
    /// The name of the reducer.
    const NAME: &'static str;

    /// A description of the parameter names of the reducer.
    const ARG_NAMES: &'static [Option<&'static str>];

    /// The function to call to invoke the reducer.
    const INVOKE: ReducerFn;
}

/// A trait for reducer types knowing their repeat interval.
pub trait RepeaterInfo: ReducerInfo {
    /// At what duration intervals should this reducer repeat?
    const REPEAT_INTERVAL: Duration;
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
    fn schema<I: ReducerInfo>(typespace: &mut impl TypespaceBuilder) -> ReducerDef;
}

/// A trait of types representing the result of executing a reducer.
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
impl<E: fmt::Debug> IntoReducerResult for Result<(), E> {
    #[inline]
    fn into_result(self) -> Result<(), Box<str>> {
        self.map_err(|e| format!("{e:?}").into())
    }
}

/// A trait of types that can be an argument of a reducer.
pub trait ReducerArg<'de> {}
impl<'de, T: Deserialize<'de>> ReducerArg<'de> for T {}
impl ReducerArg<'_> for ReducerContext {}
/// Assert that `T: ReducerArg`.
pub fn assert_reducer_arg<'de, T: ReducerArg<'de>>() {}
/// Assert that `T: IntoReducerResult`.
pub fn assert_reducer_ret<T: IntoReducerResult>() {}
pub const fn assert_reducer_typecheck<'de, A: Args<'de>, T>(_: impl Reducer<'de, A, T> + Copy) {}

/// Used in the last type parameter of `Reducer` to indicate that the
/// context argument *should* be passed to the reducer logic.
pub struct ContextArg;

/// Used in the last type parameter of `Reducer` to indicate that the
/// context argument *should not* be passed to the reducer logic.
pub struct NoContextArg;

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
            fn schema<Info: ReducerInfo>(_typespace: &mut impl TypespaceBuilder) -> ReducerDef {
                // Extract the names of the arguments.
                let [.., $($T),*] = Info::ARG_NAMES else { panic!() };
                ReducerDef {
                    name: Info::NAME.into(),
                    args: vec![
                        $(ProductTypeElement {
                            name: $T.map(Into::into),
                            algebraic_type: <$T>::make_type(_typespace),
                        }),*
                    ],
                }
            }
        }

        // Implement `Reducer<..., ContextArg>` for the tuple type `($($T,)*)`.
        impl<'de, Func, Ret, $($T: SpacetimeType + Deserialize<'de> + Serialize),*> Reducer<'de, ($($T,)*), ContextArg> for Func
        where
            Func: Fn(ReducerContext, $($T),*) -> Ret,
            Ret: IntoReducerResult
        {
            #[allow(non_snake_case)]
            fn invoke(&self, ctx: ReducerContext, args: ($($T,)*)) -> Result<(), Box<str>> {
                let ($($T,)*) = args;
                self(ctx, $($T),*).into_result()
            }

            type ArgsWithContext = (ReducerContext, $($T,)*);
            #[allow(non_snake_case, clippy::unused_unit)]
            fn extract_args(args: Self::ArgsWithContext) -> ($($T,)*) {
                let (_ctx, $($T,)*) = args;
                ($($T,)*)
            }
        }

        // Implement `Reducer<..., NoContextArg>` for the tuple type `($($T,)*)`.
        impl<'de, Func, Ret, $($T: SpacetimeType + Deserialize<'de> + Serialize),*> Reducer<'de, ($($T,)*), NoContextArg> for Func
        where
            Func: Fn($($T),*) -> Ret,
            Ret: IntoReducerResult
        {
            #[allow(non_snake_case)]
            fn invoke(&self, _ctx: ReducerContext, args: ($($T,)*)) -> Result<(), Box<str>> {
                let ($($T,)*) = args;
                self($($T),*).into_result()
            }

            type ArgsWithContext = ($($T,)*);
            fn extract_args(args: Self::ArgsWithContext) -> ($($T,)*) {
                args
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
pub fn register_table<T: TableType>() {
    register_describer(|module| {
        let data = *T::make_type(&mut module.inner).as_ref().unwrap();
        let columns: Vec<RawColumnDefV8> = RawColumnDefV8::from_product_type(
            module
                .inner
                .typespace()
                .with_type(&data)
                .resolve_refs()
                .expect("Failed to retrieve the column types from the module")
                .into_product()
                .expect("Table is not a product type"),
        );

        let indexes: Vec<_> = T::INDEXES.iter().copied().map(Into::into).collect();
        //WARNING: The definition  of table assumes the # of constraints == # of columns elsewhere `T::COLUMN_ATTRS` is queried
        let constraints: Vec<_> = T::COLUMN_ATTRS
            .iter()
            .enumerate()
            .map(|(col_pos, x)| {
                let col = &columns[col_pos];
                let kind = match (*x).try_into() {
                    Ok(x) => x,
                    Err(_) => Constraints::unset(),
                };

                RawConstraintDefV8::for_column(T::TABLE_NAME, &col.col_name, kind, ColList::new(col_pos.into()))
            })
            .collect();

        let sequences: Vec<_> = T::COLUMN_ATTRS
            .iter()
            .enumerate()
            .filter_map(|(col_pos, x)| {
                let col = &columns[col_pos];

                if x.kind() == AttributeKind::AUTO_INC {
                    Some(RawSequenceDefV8::for_column(
                        T::TABLE_NAME,
                        &col.col_name,
                        col_pos.into(),
                    ))
                } else {
                    None
                }
            })
            .collect();

        let schema = RawTableDefV8::new(T::TABLE_NAME.into(), columns)
            .with_type(StTableType::User)
            .with_access(T::TABLE_ACCESS)
            .with_constraints(constraints)
            .with_sequences(sequences)
            .with_indexes(indexes)
            .with_scheduled(T::SCHEDULED_REDUCER_NAME.map(Into::into));
        let schema = TableDesc { schema, data };

        module.inner.add_table(schema)
    })
}

impl From<crate::IndexDesc<'_>> for RawIndexDefV8 {
    fn from(index: crate::IndexDesc<'_>) -> RawIndexDefV8 {
        let columns = index.col_ids.iter().copied().collect::<ColList>();
        if columns.is_empty() {
            panic!("Need at least one column in IndexDesc for index `{}`", index.name);
        };

        RawIndexDefV8 {
            index_name: index.name.into(),
            is_unique: false,
            index_type: index.ty,
            columns,
        }
    }
}

/// Registers a describer for the reducer `I` with arguments `A`.
pub fn register_reducer<'a, A: Args<'a>, T, I: ReducerInfo>(_: impl Reducer<'a, A, T>) {
    register_describer(|module| {
        let schema = A::schema::<I>(&mut module.inner);
        module.inner.add_reducer(schema);
        module.reducers.push(I::INVOKE);
    })
}

/// A builder for a module.
#[derive(Default)]
struct ModuleBuilder {
    /// The module definition.
    inner: ModuleDefBuilder,
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
    let module_def = RawModuleDef::V8BackCompat(module_def);
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
        sender,
        timestamp,
        address,
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
    let mut buf = take_iter_buf();

    // Read `args` and run `logic`.
    read_bytes_source_into(args, &mut buf);
    let ret = logic(&buf);

    // Return the `buf` back to the pool.
    // Should a panic occur before reaching here,
    // the WASM module cannot recover and will trap,
    // so we don't need to care that this is not returned to the pool.
    return_iter_buf(buf);
    ret
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

#[cfg(feature = "unstable_abi")]
#[doc(hidden)]
pub fn volatile_nonatomic_schedule_immediate<'de, A: Args<'de>, R: Reducer<'de, A, T>, R2: ReducerInfo, T>(
    _reducer: R,
    args: R::ArgsWithContext,
) {
    let args = R::extract_args(args);
    let arg_bytes = bsatn::to_vec(&SerDeArgs(args)).unwrap();

    // Schedule the reducer.
    sys::volatile_nonatomic_schedule_immediate(R2::NAME, &arg_bytes)
}
