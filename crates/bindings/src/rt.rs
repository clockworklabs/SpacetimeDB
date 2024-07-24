#![deny(unsafe_op_in_unsafe_fn)]

use std::fmt;
use std::marker::PhantomData;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use sys::Buffer;

use crate::timestamp::with_timestamp_set;
use crate::{sys, ReducerContext, SpacetimeType, TableType, Timestamp};
use spacetimedb_lib::db::auth::StTableType;
use spacetimedb_lib::db::raw_def::{
    RawColumnDefV8, RawConstraintDefV8, RawIndexDefV8, RawSequenceDefV8, RawTableDefV8,
};
use spacetimedb_lib::de::{self, Deserialize, SeqProductAccess};
use spacetimedb_lib::sats::typespace::TypespaceBuilder;
use spacetimedb_lib::sats::{impl_deserialize, impl_serialize, ProductTypeElement};
use spacetimedb_lib::ser::{Serialize, SerializeSeqProduct};
use spacetimedb_lib::{bsatn, Address, Identity, ModuleDefBuilder, ReducerDef, TableDesc};
use spacetimedb_primitives::*;

/// The `sender` invokes `reducer` at `timestamp` and provides it with the given `args`.
///
/// Returns an invalid buffer on success
/// and otherwise the error is written into the fresh one returned.
pub fn invoke_reducer<'a, A: Args<'a>, T>(
    reducer: impl Reducer<'a, A, T>,
    sender: Buffer,
    client_address: Buffer,
    timestamp: u64,
    args: &'a [u8],
) -> Buffer {
    let ctx = assemble_context(sender, timestamp, client_address);

    // Deserialize the arguments from a bsatn encoding.
    let SerDeArgs(args) = bsatn::from_slice(args).expect("unable to decode args");

    // Run the reducer with the environment all set up.
    let invoke = || reducer.invoke(ctx, args);
    #[cfg(feature = "rand")]
    let invoke = || crate::rng::with_rng_set(invoke);
    let res = with_timestamp_set(ctx.timestamp, invoke);

    // Any error is pushed into a `Buffer`.
    cvt_result(res)
}

/// Creates an index with the name `index_name` and type `index_type`,
/// on a product of the given columns ids in `col_ids`,
/// identifying columns in the table identified by `table_id`.
///
/// Currently only single-column-indices are supported
/// and they may only be of the btree index type.
/// Attempting to create a multi-column index will result in a panic.
/// Attempting to use an index type other than btree, meanwhile, will return an error.
///
/// Returns an invalid buffer on success
/// and otherwise the error is written into the fresh one returned
/// when `table_id` doesn't identify a table.
pub fn create_index(index_name: &str, table_id: TableId, index_type: sys::raw::IndexType, col_ids: Vec<u8>) -> Buffer {
    let result = sys::create_index(index_name, table_id, index_type as u8, &col_ids);
    cvt_result(result.map_err(cvt_errno))
}

/// Creates a reducer context from the given `sender`, `timestamp` and `client_address`.
///
/// `sender` must contain 32 bytes, from which we will read an `Identity`.
///
/// `timestamp` is a count of microseconds since the Unix epoch.
///
/// `client_address` must contain 16 bytes, from which we will read an `Address`.
/// The all-zeros `client_address` (constructed by [`Address::__dummy`]) is used as a sentinel,
/// and translated to `None`.
fn assemble_context(sender: Buffer, timestamp: u64, client_address: Buffer) -> ReducerContext {
    let sender = Identity::from_byte_array(sender.read_array::<32>());

    let timestamp = Timestamp::UNIX_EPOCH + Duration::from_micros(timestamp);

    let address = Address::from_arr(&client_address.read_array::<16>());

    let address = if address == Address::__DUMMY {
        None
    } else {
        Some(address)
    };

    ReducerContext {
        sender,
        timestamp,
        address,
    }
}

/// Converts `errno` into a string message.
fn cvt_errno(errno: sys::Errno) -> Box<str> {
    let message = format!("{errno}");
    message.into_boxed_str()
}

/// Converts `res` into a `Buffer` where `Ok(_)` results in an invalid buffer
/// and an error message is moved into a fresh buffer.
fn cvt_result(res: Result<(), Box<str>>) -> Buffer {
    match res {
        Ok(()) => Buffer::INVALID,
        Err(errmsg) => Buffer::alloc(errmsg.as_bytes()),
    }
}

/// A trait for types representing the *execution logic* of a reducer.
///
/// The type parameter `T` is used for determining whether there is a context argument.
pub trait Reducer<'de, A: Args<'de>, T> {
    fn invoke(&self, ctx: ReducerContext, args: A) -> Result<(), Box<str>>;
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
pub trait ReducerResult {
    /// Convert the result into form where there is no value
    /// and the error message is a string.
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

/// A trait of types that can be an argument of a reducer.
pub trait ReducerArg<'de> {}
impl<'de, T: Deserialize<'de>> ReducerArg<'de> for T {}
impl ReducerArg<'_> for ReducerContext {}
/// Assert that `T: ReducerArg`.
pub fn assert_reducer_arg<'de, T: ReducerArg<'de>>() {}
/// Assert that `T: ReducerResult`.
pub fn assert_reducer_ret<T: ReducerResult>() {}
/// Assert that `T: TableType`.
pub const fn assert_table<T: TableType>() {}

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

            fn serialize_seq_product<Ser: SerializeSeqProduct>(&self, _prod: &mut Ser) -> Result<(), Ser::Error> {
                // For every element in the product, serialize.
                #[allow(non_snake_case)]
                let ($($T,)*) = self;
                $(_prod.serialize_element($T)?;)*
                Ok(())
            }

            #[inline]
            fn schema<Info: ReducerInfo>(_typespace: &mut impl TypespaceBuilder) -> ReducerDef {
                // Extract the names of the arguments.
                #[allow(non_snake_case, irrefutable_let_patterns)]
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
            Ret: ReducerResult
        {
            fn invoke(&self, ctx: ReducerContext, args: ($($T,)*)) -> Result<(), Box<str>> {
                #[allow(non_snake_case)]
                let ($($T,)*) = args;
                self(ctx, $($T),*).into_result()
            }
        }

        // Implement `Reducer<..., NoContextArg>` for the tuple type `($($T,)*)`.
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
                .and_then(|x| x.into_product().ok())
                .expect("Fail to retrieve the columns from the module"),
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
        let Ok(columns) = index
            .col_ids
            .iter()
            .map(|x| (*x).into())
            .collect::<ColListBuilder>()
            .build()
        else {
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

/// A reducer function takes in `(Sender, Timestamp, Args)` and writes to a new `Buffer`.
pub type ReducerFn = fn(Buffer, Buffer, u64, &[u8]) -> Buffer;
static REDUCERS: OnceLock<Vec<ReducerFn>> = OnceLock::new();

/// Describes the module into a serialized form that is returned and writes the set of `REDUCERS`.
#[no_mangle]
extern "C" fn __describe_module__() -> Buffer {
    // Collect the `module`.
    let mut module = ModuleBuilder::default();
    for describer in &*DESCRIBERS.lock().unwrap() {
        describer(&mut module)
    }

    // Serialize the module to bsatn.
    let module_def = module.inner.finish();
    let bytes = bsatn::to_vec(&module_def).expect("unable to serialize typespace");

    // Write the set of reducers.
    REDUCERS.set(module.reducers).ok().unwrap();

    // Allocate the bsatn data into a fresh buffer.
    Buffer::alloc(&bytes)
}

/// The `sender` calls the reducer identified by `id` at `timestamp` with `args`.
///
/// The result of the reducer is written into a fresh buffer.
#[no_mangle]
extern "C" fn __call_reducer__(
    id: usize,
    sender: Buffer,
    caller_address: Buffer,
    timestamp: u64,
    args: Buffer,
) -> Buffer {
    let reducers = REDUCERS.get().unwrap();
    let args = args.read();
    reducers[id](sender, caller_address, timestamp, &args)
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
