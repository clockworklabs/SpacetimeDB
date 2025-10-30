#![deny(unsafe_op_in_unsafe_fn)]

use crate::table::IndexAlgo;
use crate::{
    sys, AnonymousViewContext, IterBuf, LocalReadOnly, ProcedureContext, ProcedureResult, ReducerContext,
    ReducerResult, SpacetimeType, Table, ViewContext,
};
pub use spacetimedb_lib::db::raw_def::v9::Lifecycle as LifecycleReducer;
use spacetimedb_lib::db::raw_def::v9::{RawIndexAlgorithm, RawModuleDefV9Builder, TableType};
use spacetimedb_lib::de::{self, Deserialize, Error as _, SeqProductAccess};
use spacetimedb_lib::sats::typespace::TypespaceBuilder;
use spacetimedb_lib::sats::{impl_deserialize, impl_serialize, ProductTypeElement};
use spacetimedb_lib::ser::{Serialize, SerializeSeqProduct};
use spacetimedb_lib::{bsatn, AlgebraicType, ConnectionId, Identity, ProductType, RawModuleDef, Timestamp};
use spacetimedb_primitives::*;
use std::convert::Infallible;
use std::fmt;
use std::marker::PhantomData;
use std::sync::{Mutex, OnceLock};
use sys::raw::{BytesSink, BytesSource};

pub trait IntoVec<T> {
    fn into_vec(self) -> Vec<T>;
}

impl<T> IntoVec<T> for Vec<T> {
    fn into_vec(self) -> Vec<T> {
        self
    }
}

impl<T> IntoVec<T> for Option<T> {
    fn into_vec(self) -> Vec<T> {
        self.into_iter().collect()
    }
}

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

    reducer.invoke(&ctx, args)
}

pub fn invoke_procedure<'a, A: Args<'a>, Ret: IntoProcedureResult>(
    procedure: impl Procedure<'a, A, Ret>,
    mut ctx: ProcedureContext,
    args: &'a [u8],
) -> ProcedureResult {
    // Deserialize the arguments from a bsatn encoding.
    let SerDeArgs(args) = bsatn::from_slice(args).expect("unable to decode args");

    // TODO(procedure-async): get a future out of `procedure.invoke` and call `FutureExt::now_or_never` on it?
    // Or maybe do that within the `Procedure::invoke` method?
    let res = procedure.invoke(&mut ctx, args);

    res.to_result()
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

/// Invoke a caller-specific view.
/// Returns a BSATN encoded `Vec` of rows.
pub fn invoke_view<'a, A: Args<'a>, T: SpacetimeType + Serialize>(
    view: impl View<'a, A, T>,
    ctx: ViewContext,
    args: &'a [u8],
) -> Vec<u8> {
    // Deserialize the arguments from a bsatn encoding.
    let SerDeArgs(args) = bsatn::from_slice(args).expect("unable to decode args");
    let rows: Vec<T> = view.invoke(&ctx, args);
    let mut buf = IterBuf::take();
    buf.serialize_into(&rows).expect("unable to encode rows");
    std::mem::take(&mut *buf)
}
/// A trait for types representing the execution logic of a caller-specific view.
#[diagnostic::on_unimplemented(
    message = "invalid view signature",
    label = "this view signature is not valid",
    note = "",
    note = "view signatures must match:",
    note = "    `Fn(&ViewContext, [T1, ...]) -> Vec<Tn> | Option<Tn>`",
    note = "where each `Ti` implements `SpacetimeType`.",
    note = ""
)]
pub trait View<'de, A: Args<'de>, T: SpacetimeType + Serialize> {
    fn invoke(&self, ctx: &ViewContext, args: A) -> Vec<T>;
}

/// Invoke an anonymous view.
/// Returns a BSATN encoded `Vec` of rows.
pub fn invoke_anonymous_view<'a, A: Args<'a>, T: SpacetimeType + Serialize>(
    view: impl AnonymousView<'a, A, T>,
    ctx: AnonymousViewContext,
    args: &'a [u8],
) -> Vec<u8> {
    // Deserialize the arguments from a bsatn encoding.
    let SerDeArgs(args) = bsatn::from_slice(args).expect("unable to decode args");
    let rows: Vec<T> = view.invoke(&ctx, args);
    let mut buf = IterBuf::take();
    buf.serialize_into(&rows).expect("unable to encode rows");
    std::mem::take(&mut *buf)
}
/// A trait for types representing the execution logic of an anonymous view.
#[diagnostic::on_unimplemented(
    message = "invalid anonymous view signature",
    label = "this view signature is not valid",
    note = "",
    note = "anonymous view signatures must match:",
    note = "    `Fn(&AnonymousViewContext, [T1, ...]) -> Vec<Tn> | Option<Tn>`",
    note = "where each `Ti` implements `SpacetimeType`.",
    note = ""
)]
pub trait AnonymousView<'de, A: Args<'de>, T: SpacetimeType + Serialize> {
    fn invoke(&self, ctx: &AnonymousViewContext, args: A) -> Vec<T>;
}

/// A trait for types that can *describe* a callable function such as a reducer or view.
pub trait FnInfo {
    /// The type of function to invoke.
    type Invoke;

    /// One of [`FnKindReducer`], [`FnKindProcedure`] or [`FnKindView`].
    ///
    /// Used as a type argument to [`ExportFunctionForScheduledTable`] and [`scheduled_typecheck`].
    /// See <https://willcrichton.net/notes/defeating-coherence-rust/> for details on this technique.
    type FnKind;

    /// The name of the function.
    const NAME: &'static str;

    /// The lifecycle of the function, if there is one.
    const LIFECYCLE: Option<LifecycleReducer> = None;

    /// A description of the parameter names of the function.
    const ARG_NAMES: &'static [Option<&'static str>];

    /// The function to invoke.
    const INVOKE: Self::Invoke;

    /// The return type of this function.
    /// Currently only implemented for views.
    fn return_type(_ts: &mut impl TypespaceBuilder) -> Option<AlgebraicType> {
        None
    }
}

pub trait Procedure<'de, A: Args<'de>, Ret: IntoProcedureResult> {
    fn invoke(&self, ctx: &mut ProcedureContext, args: A) -> Ret;
}

/// A trait of types representing the arguments of a reducer, procedure or view.
///
/// This does not include the context first argument,
/// only the client-provided args.
/// As such, the same trait can be used for all sorts of exported functions.
pub trait Args<'de>: Sized {
    /// How many arguments does the reducer accept?
    const LEN: usize;

    /// Deserialize the arguments from the sequence `prod` which knows when there are next elements.
    fn visit_seq_product<A: SeqProductAccess<'de>>(prod: A) -> Result<Self, A::Error>;

    /// Serialize the arguments in `self` into the sequence `prod` according to the type `S`.
    fn serialize_seq_product<S: SerializeSeqProduct>(&self, prod: &mut S) -> Result<(), S::Error>;

    /// Returns the schema of the args for this function provided a `typespace`.
    fn schema<I: FnInfo>(typespace: &mut impl TypespaceBuilder) -> ProductType;
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
    message = "The procedure return type `{Self}` does not implement `SpacetimeType`",
    note = "if you own the type, try adding `#[derive(SpacetimeType)]` to its definition"
)]
pub trait IntoProcedureResult: SpacetimeType + Serialize {
    #[inline]
    fn to_result(&self) -> ProcedureResult {
        bsatn::to_vec(&self).expect("Failed to serialize procedure result")
    }
}
impl<T: SpacetimeType + Serialize> IntoProcedureResult for T {}

#[diagnostic::on_unimplemented(
    message = "the first argument of a reducer must be `&ReducerContext`",
    label = "first argument must be `&ReducerContext`"
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

#[diagnostic::on_unimplemented(
    message = "the first argument of a procedure must be `&mut ProcedureContext`",
    label = "first argument must be `&mut ProcedureContext`"
)]
pub trait ProcedureContextArg {
    // a little hack used in the macro to make error messages nicer. it generates <T as ReducerContextArg>::_ITEM
    #[doc(hidden)]
    const _ITEM: () = ();
}
impl ProcedureContextArg for &mut ProcedureContext {}

/// A trait of types that can be an argument of a procedure.
#[diagnostic::on_unimplemented(
    message = "the procedure argument `{Self}` does not implement `SpacetimeType`",
    note = "if you own the type, try adding `#[derive(SpacetimeType)]` to its definition"
)]
pub trait ProcedureArg {
    // a little hack used in the macro to make error messages nicer. it generates <T as ReducerArg>::_ITEM
    #[doc(hidden)]
    const _ITEM: () = ();
}
impl<T: SpacetimeType> ProcedureArg for T {}

#[diagnostic::on_unimplemented(
    message = "The first parameter of a `#[view]` must be `&ViewContext` or `&AnonymousViewContext`"
)]
pub trait ViewContextArg {
    #[doc(hidden)]
    const _ITEM: () = ();
}
impl ViewContextArg for ViewContext {}
impl ViewContextArg for AnonymousViewContext {}

/// A trait of types that can be an argument of a view.
#[diagnostic::on_unimplemented(
    message = "the view argument `{Self}` does not implement `SpacetimeType`",
    note = "if you own the type, try adding `#[derive(SpacetimeType)]` to its definition"
)]
pub trait ViewArg {
    #[doc(hidden)]
    const _ITEM: () = ();
}
impl<T: SpacetimeType> ViewArg for T {}

/// A trait of types that can be the return type of a view.
#[diagnostic::on_unimplemented(message = "Views must return `Vec<T>` or `Option<T>` where `T` is a `SpacetimeType`")]
pub trait ViewReturn {
    #[doc(hidden)]
    const _ITEM: () = ();
}
impl<T: SpacetimeType> ViewReturn for Vec<T> {}
impl<T: SpacetimeType> ViewReturn for Option<T> {}

/// Map the correct dispatcher based on the `Ctx` type
pub struct ViewKind<Ctx> {
    _marker: PhantomData<Ctx>,
}

pub trait ViewKindTrait {
    type InvokeFn;
}

impl ViewKindTrait for ViewKind<ViewContext> {
    type InvokeFn = ViewFn;
}

impl ViewKindTrait for ViewKind<AnonymousViewContext> {
    type InvokeFn = AnonymousFn;
}

/// Invoke the correct dispatcher based on the `Ctx` type
pub struct ViewDispatcher<Ctx> {
    _marker: PhantomData<Ctx>,
}

impl ViewDispatcher<ViewContext> {
    #[inline]
    pub fn invoke<'a, A, T, V>(view: V, ctx: ViewContext, args: &'a [u8]) -> Vec<u8>
    where
        A: Args<'a>,
        T: SpacetimeType + Serialize,
        V: View<'a, A, T>,
    {
        invoke_view(view, ctx, args)
    }
}

impl ViewDispatcher<AnonymousViewContext> {
    #[inline]
    pub fn invoke<'a, A, T, V>(view: V, ctx: AnonymousViewContext, args: &'a [u8]) -> Vec<u8>
    where
        A: Args<'a>,
        T: SpacetimeType + Serialize,
        V: AnonymousView<'a, A, T>,
    {
        invoke_anonymous_view(view, ctx, args)
    }
}

/// Register the correct dispatcher based on the `Ctx` type
pub struct ViewRegistrar<Ctx> {
    _marker: PhantomData<Ctx>,
}

impl ViewRegistrar<ViewContext> {
    #[inline]
    pub fn register<'a, A, I, T, V>(view: V)
    where
        A: Args<'a>,
        T: SpacetimeType + Serialize,
        I: FnInfo<Invoke = ViewFn>,
        V: View<'a, A, T>,
    {
        register_view::<A, I, T>(view)
    }
}

impl ViewRegistrar<AnonymousViewContext> {
    #[inline]
    pub fn register<'a, A, I, T, V>(view: V)
    where
        A: Args<'a>,
        T: SpacetimeType + Serialize,
        I: FnInfo<Invoke = AnonymousFn>,
        V: AnonymousView<'a, A, T>,
    {
        register_anonymous_view::<A, I, T>(view)
    }
}

/// Assert that a reducer type-checks with a given type.
pub const fn scheduled_typecheck<'de, Row, FnKind>(_x: impl ExportFunctionForScheduledTable<'de, Row, FnKind>)
where
    Row: SpacetimeType + Serialize + Deserialize<'de>,
{
    core::mem::forget(_x);
}

/// Tacit marker argument to [`ExportFunctionForScheduledTable`] for reducers.
pub struct FnKindReducer {
    _never: Infallible,
}

/// Tacit marker argument to [`ExportFunctionForScheduledTable`] for procedures.
///
/// Holds the procedure's return type in order to avoid an error due to an unconstrained type argument.
pub struct FnKindProcedure<Ret> {
    _never: Infallible,
    _ret_ty: PhantomData<fn() -> Ret>,
}

/// Tacit marker argument to [`ExportFunctionForScheduledTable`] for views.
///
/// Because views are never scheduled, we don't need to distinguish between anonymous or sender-identity views,
/// or to include their return type.
pub struct FnKindView {
    _never: Infallible,
}

/// Trait bound for [`scheduled_typecheck`], which the [`crate::table`] macro generates to typecheck scheduled functions.
///
/// The `FnKind` parameter here is a coherence-defeating marker, which Will Crichton calls a "tacit parameter."
/// See <https://willcrichton.net/notes/defeating-coherence-rust/> for details on this technique.
/// It will be one of [`FnKindReducer`] or [`FnKindProcedure`] in modules that compile successfully.
/// It may be [`FnKindView`], but that will always fail to typecheck, as views cannot be used as scheduled functions.
#[diagnostic::on_unimplemented(
    message = "invalid signature for scheduled table reducer or procedure",
    note = "views cannot be scheduled",
    note = "the scheduled function must take `{TableRow}` as its sole argument",
    note = "e.g: `fn scheduled_reducer(ctx: &ReducerContext, arg: {TableRow})`",
    // TODO(procedure-async): amend this to `async fn` once procedures are `async`-ified
    note = "or `fn scheduled_procedure(ctx: &mut ProcedureContext, arg: {TableRow})`",
)]
pub trait ExportFunctionForScheduledTable<'de, TableRow, FnKind> {}
impl<'de, TableRow: SpacetimeType + Serialize + Deserialize<'de>, F: Reducer<'de, (TableRow,)>>
    ExportFunctionForScheduledTable<'de, TableRow, FnKindReducer> for F
{
}

impl<
        'de,
        TableRow: SpacetimeType + Serialize + Deserialize<'de>,
        Ret: SpacetimeType + Serialize + Deserialize<'de>,
        F: Procedure<'de, (TableRow,), Ret>,
    > ExportFunctionForScheduledTable<'de, TableRow, FnKindProcedure<Ret>> for F
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

/// Assert that the primary_key column of a scheduled table is a u64.
pub const fn assert_scheduled_table_primary_key<T: ScheduledTablePrimaryKey>() {}

mod sealed {
    pub trait Sealed {}
}
#[diagnostic::on_unimplemented(
    message = "scheduled table primary key must be a `u64`",
    label = "should be `u64`, not `{Self}`"
)]
pub trait ScheduledTablePrimaryKey: sealed::Sealed {}
impl sealed::Sealed for u64 {}
impl ScheduledTablePrimaryKey for u64 {}

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
        Err(Acc::Error::named_products_not_supported())
    }
}

macro_rules! impl_reducer_procedure_view {
    ($($T1:ident $(, $T:ident)*)?) => {
        impl_reducer_procedure_view!(@impl $($T1 $(, $T)*)?);
        $(impl_reducer_procedure_view!($($T),*);)?
    };
    (@impl $($T:ident),*) => {
        // Implement `Args` for the tuple type `($($T,)*)`.
        impl<'de, $($T: SpacetimeType + Deserialize<'de> + Serialize),*> Args<'de> for ($($T,)*) {
            const LEN: usize = impl_reducer_procedure_view!(@count $($T)*);
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
            fn schema<Info: FnInfo>(_typespace: &mut impl TypespaceBuilder) -> ProductType {
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

        impl<'de, Func, Ret, $($T: SpacetimeType + Deserialize<'de> + Serialize),*> Procedure<'de, ($($T,)*), Ret> for Func
        where
            Func: Fn(&mut ProcedureContext, $($T),*) -> Ret,
            Ret: IntoProcedureResult,
        {
            #[allow(non_snake_case)]
            fn invoke(&self, ctx: &mut ProcedureContext, args: ($($T,)*)) -> Ret {
                let ($($T,)*) = args;
                self(ctx, $($T),*)
            }
        }

        // Implement `View<..., ViewContext>` for the tuple type `($($T,)*)`.
        impl<'de, Func, Elem, Retn, $($T),*>
            View<'de, ($($T,)*), Elem> for Func
        where
            $($T: SpacetimeType + Deserialize<'de> + Serialize,)*
            Func: Fn(&ViewContext, $($T),*) -> Retn,
            Retn: IntoVec<Elem>,
            Elem: SpacetimeType + Serialize,
        {
            #[allow(non_snake_case)]
            fn invoke(&self, ctx: &ViewContext, args: ($($T,)*)) -> Vec<Elem> {
                let ($($T,)*) = args;
                self(ctx, $($T),*).into_vec()
            }
        }

        // Implement `View<..., AnonymousViewContext>` for the tuple type `($($T,)*)`.
        impl<'de, Func, Elem, Retn, $($T),*>
            AnonymousView<'de, ($($T,)*), Elem> for Func
        where
            $($T: SpacetimeType + Deserialize<'de> + Serialize,)*
            Func: Fn(&AnonymousViewContext, $($T),*) -> Retn,
            Retn: IntoVec<Elem>,
            Elem: SpacetimeType + Serialize,
        {
            #[allow(non_snake_case)]
            fn invoke(&self, ctx: &AnonymousViewContext, args: ($($T,)*)) -> Vec<Elem> {
                let ($($T,)*) = args;
                self(ctx, $($T),*).into_vec()
            }
        }
    };
    // Counts the number of elements in the tuple.
    (@count $($T:ident)*) => {
        0 $(+ impl_reducer_procedure_view!(@drop $T 1))*
    };
    (@drop $a:tt $b:tt) => { $b };
}

impl_reducer_procedure_view!(
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z, AA, AB, AC, AD, AE, AF
);

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

/// A trait for types that can *describe* a row-level security policy.
pub trait RowLevelSecurityInfo {
    /// The SQL expression for the row-level security policy.
    const SQL: &'static str;
}

/// A function which will be registered by [`register_describer`] into [`DESCRIBERS`],
/// which will be called by [`__describe_module__`] to construct a module definition.
///
/// May be a closure over static data, so that e.g.
/// [`register_row_level_security`] doesn't need to take a type parameter.
/// Permitted by the type system to be a [`FnMut`] mutable closure,
/// since [`DESCRIBERS`] is in a [`Mutex`] anyways,
/// but will likely cause weird misbehaviors if a non-idempotent function is used.
trait DescriberFn: FnMut(&mut ModuleBuilder) + Send + 'static {}
impl<F: FnMut(&mut ModuleBuilder) + Send + 'static> DescriberFn for F {}

/// Registers into `DESCRIBERS` a function `f` to modify the module builder.
fn register_describer(f: impl DescriberFn) {
    DESCRIBERS.lock().unwrap().push(Box::new(f))
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
            table = table.with_unique_constraint(col);
        }
        for &index in T::INDEXES {
            table = table.with_index(index.algo.into(), index.accessor_name);
        }
        if let Some(primary_key) = T::PRIMARY_KEY {
            table = table.with_primary_key(primary_key);
        }
        for &col in T::SEQUENCES {
            table = table.with_column_sequence(col);
        }
        if let Some(schedule) = T::SCHEDULE {
            table = table.with_schedule(schedule.reducer_or_procedure_name, schedule.scheduled_at_column);
        }

        for col in T::get_default_col_values().iter_mut() {
            table = table.with_default_column_value(col.col_id, col.value.clone())
        }

        table.finish();
    })
}

impl From<IndexAlgo<'_>> for RawIndexAlgorithm {
    fn from(algo: IndexAlgo<'_>) -> RawIndexAlgorithm {
        match algo {
            IndexAlgo::BTree { columns } => RawIndexAlgorithm::BTree {
                columns: columns.iter().copied().collect(),
            },
            IndexAlgo::Direct { column } => RawIndexAlgorithm::Direct { column: column.into() },
        }
    }
}

/// Registers a describer for the reducer `I` with arguments `A`.
pub fn register_reducer<'a, A: Args<'a>, I: FnInfo<Invoke = ReducerFn>>(_: impl Reducer<'a, A>) {
    register_describer(|module| {
        let params = A::schema::<I>(&mut module.inner);
        module.inner.add_reducer(I::NAME, params, I::LIFECYCLE);
        module.reducers.push(I::INVOKE);
    })
}

pub fn register_procedure<'a, A, Ret, I>(_: impl Procedure<'a, A, Ret>)
where
    A: Args<'a>,
    Ret: SpacetimeType + Serialize,
    I: FnInfo<Invoke = ProcedureFn>,
{
    register_describer(|module| {
        let params = A::schema::<I>(&mut module.inner);
        let ret_ty = <Ret as SpacetimeType>::make_type(&mut module.inner);
        module.inner.add_procedure(I::NAME, params, ret_ty);
        module.procedures.push(I::INVOKE);
    })
}

/// Registers a describer for the view `I` with arguments `A` and return type `Vec<T>`.
pub fn register_view<'a, A, I, T>(_: impl View<'a, A, T>)
where
    A: Args<'a>,
    I: FnInfo<Invoke = ViewFn>,
    T: SpacetimeType + Serialize,
{
    register_describer(|module| {
        let params = A::schema::<I>(&mut module.inner);
        let return_type = I::return_type(&mut module.inner).unwrap();
        module.inner.add_view(I::NAME, true, false, params, return_type);
        module.views.push(I::INVOKE);
    })
}

/// Registers a describer for the anonymous view `I` with arguments `A` and return type `Vec<T>`.
pub fn register_anonymous_view<'a, A, I, T>(_: impl AnonymousView<'a, A, T>)
where
    A: Args<'a>,
    I: FnInfo<Invoke = AnonymousFn>,
    T: SpacetimeType + Serialize,
{
    register_describer(|module| {
        let params = A::schema::<I>(&mut module.inner);
        let return_type = I::return_type(&mut module.inner).unwrap();
        module.inner.add_view(I::NAME, true, true, params, return_type);
        module.views_anon.push(I::INVOKE);
    })
}

/// Registers a row-level security policy.
pub fn register_row_level_security(sql: &'static str) {
    register_describer(|module| {
        module.inner.add_row_level_security(sql);
    })
}

/// A builder for a module.
#[derive(Default)]
pub struct ModuleBuilder {
    /// The module definition.
    inner: RawModuleDefV9Builder,
    /// The reducers of the module.
    reducers: Vec<ReducerFn>,
    /// The procedures of the module.
    procedures: Vec<ProcedureFn>,
    /// The client specific views of the module.
    views: Vec<ViewFn>,
    /// The anonymous views of the module.
    views_anon: Vec<AnonymousFn>,
}

// Not actually a mutex; because WASM is single-threaded this basically just turns into a refcell.
static DESCRIBERS: Mutex<Vec<Box<dyn DescriberFn>>> = Mutex::new(Vec::new());

/// A reducer function takes in `(ReducerContext, Args)`
/// and returns a result with a possible error message.
pub type ReducerFn = fn(ReducerContext, &[u8]) -> ReducerResult;
static REDUCERS: OnceLock<Vec<ReducerFn>> = OnceLock::new();

pub type ProcedureFn = fn(ProcedureContext, &[u8]) -> ProcedureResult;
static PROCEDURES: OnceLock<Vec<ProcedureFn>> = OnceLock::new();

/// A view function takes in `(ViewContext, Args)` and returns a Vec of bytes.
pub type ViewFn = fn(ViewContext, &[u8]) -> Vec<u8>;
static VIEWS: OnceLock<Vec<ViewFn>> = OnceLock::new();

/// An anonymous view function takes in `(AnonymousViewContext, Args)` and returns a Vec of bytes.
pub type AnonymousFn = fn(AnonymousViewContext, &[u8]) -> Vec<u8>;
static ANONYMOUS_VIEWS: OnceLock<Vec<AnonymousFn>> = OnceLock::new();

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
/// The `ModuleDef` is used to define tables, constraints, indexes, reducers, etc.
/// This affords the module the opportunity
/// to define and, to a limited extent, alter the schema at initialization time,
/// including when modules are updated (re-publishing).
/// After initialization, the module cannot alter the schema.
#[no_mangle]
extern "C" fn __describe_module__(description: BytesSink) {
    // Collect the `module`.
    let mut module = ModuleBuilder::default();
    for describer in &mut *DESCRIBERS.lock().unwrap() {
        describer(&mut module)
    }

    // Serialize the module to bsatn.
    let module_def = module.inner.finish();
    let module_def = RawModuleDef::V9(module_def);
    let bytes = bsatn::to_vec(&module_def).expect("unable to serialize typespace");

    // Write the sets of reducers, procedures and views.
    REDUCERS.set(module.reducers).ok().unwrap();
    PROCEDURES.set(module.procedures).ok().unwrap();
    VIEWS.set(module.views).ok().unwrap();
    ANONYMOUS_VIEWS.set(module.views_anon).ok().unwrap();

    // Write the bsatn data into the sink.
    write_to_sink(description, &bytes);
}

// TODO(1.0): update `__call_reducer__` docs + for `BytesSink`.

/// Called by the host to execute a reducer
/// when the `sender` calls the reducer identified by `id` at `timestamp` with `args`.
///
/// The `sender_{0-3}` are the pieces of a `[u8; 32]` (`u256`) representing the sender's `Identity`.
/// They are encoded as follows (assuming `identity.to_byte_array(): [u8; 32]`):
/// - `sender_0` contains bytes `[0 ..8 ]`.
/// - `sender_1` contains bytes `[8 ..16]`.
/// - `sender_2` contains bytes `[16..24]`.
/// - `sender_3` contains bytes `[24..32]`.
///
/// Note that `to_byte_array` uses LITTLE-ENDIAN order! This matches most host systems.
///
/// The `conn_id_{0-1}` are the pieces of a `[u8; 16]` (`u128`) representing the callers's [`ConnectionId`].
/// They are encoded as follows (assuming `conn_id.as_le_byte_array(): [u8; 16]`):
/// - `conn_id_0` contains bytes `[0 ..8 ]`.
/// - `conn_id_1` contains bytes `[8 ..16]`.
///
/// Again, note that `to_byte_array` uses LITTLE-ENDIAN order! This matches most host systems.
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
    conn_id_0: u64,
    conn_id_1: u64,
    timestamp: u64,
    args: BytesSource,
    error: BytesSink,
) -> i16 {
    // Piece together `sender_i` into an `Identity`.
    let sender = reconstruct_sender_identity(sender_0, sender_1, sender_2, sender_3);

    // Piece together `conn_id_i` into a `ConnectionId`.
    // The all-zeros `ConnectionId` (`ConnectionId::ZERO`) is interpreted as `None`.
    let conn_id = reconstruct_connection_id(conn_id_0, conn_id_1);

    // Assemble the `ReducerContext`.
    let timestamp = Timestamp::from_micros_since_unix_epoch(timestamp as i64);
    let ctx = ReducerContext::new(crate::Local {}, sender, conn_id, timestamp);

    // Fetch reducer function.
    let reducers = REDUCERS.get().unwrap();
    // Dispatch to it with the arguments read.
    let res = with_read_args(args, |args| reducers[id](ctx, args));
    // Convert any error message to an error code and writes to the `error` sink.
    convert_err_to_errno(res, error)
}

/// Reconstruct the `sender_i` args to [`__call_reducer__`] and [`__call_procedure__`] into an [`Identity`].
fn reconstruct_sender_identity(sender_0: u64, sender_1: u64, sender_2: u64, sender_3: u64) -> Identity {
    let sender = [sender_0, sender_1, sender_2, sender_3];
    let sender: [u8; 32] = bytemuck::must_cast(sender);
    Identity::from_byte_array(sender) // The LITTLE-ENDIAN constructor.
}

/// Reconstruct the `conn_id_i` args to [`__call_reducer__`] and [`__call_procedure__`] into a [`ConnectionId`].
///
/// The all-zeros `ConnectionId` (`ConnectionId::ZERO`) is interpreted as `None`.
fn reconstruct_connection_id(conn_id_0: u64, conn_id_1: u64) -> Option<ConnectionId> {
    // Piece together `conn_id_i` into a `ConnectionId`.
    // The all-zeros `ConnectionId` (`ConnectionId::ZERO`) is interpreted as `None`.
    let conn_id = [conn_id_0, conn_id_1];
    let conn_id: [u8; 16] = bytemuck::must_cast(conn_id);
    let conn_id = ConnectionId::from_le_byte_array(conn_id); // The LITTLE-ENDIAN constructor.
    (conn_id != ConnectionId::ZERO).then_some(conn_id)
}

/// If `res` is `Err`, write the message to `out` and return non-zero.
/// If `res` is `Ok`, return zero.
///
/// Called by [`__call_reducer__`] and [`__call_procedure__`]
/// to convert the user-returned `Result` into a low-level errno return.
fn convert_err_to_errno(res: Result<(), Box<str>>, out: BytesSink) -> i16 {
    match res {
        Ok(()) => 0,
        Err(msg) => {
            write_to_sink(out, msg.as_bytes());
            errno::HOST_CALL_FAILURE.get() as i16
        }
    }
}

/// Called by the host to execute a procedure
/// when the `sender` calls the procedure identified by `id` at `timestamp` with `args`.
///
/// The `sender_{0-3}` are the pieces of a `[u8; 32]` (`u256`) representing the sender's `Identity`.
/// They are encoded as follows (assuming `identity.to_byte_array(): [u8; 32]`):
/// - `sender_0` contains bytes `[0 ..8 ]`.
/// - `sender_1` contains bytes `[8 ..16]`.
/// - `sender_2` contains bytes `[16..24]`.
/// - `sender_3` contains bytes `[24..32]`.
///
/// Note that `to_byte_array` uses LITTLE-ENDIAN order! This matches most host systems.
///
/// The `conn_id_{0-1}` are the pieces of a `[u8; 16]` (`u128`) representing the callers's [`ConnectionId`].
/// They are encoded as follows (assuming `conn_id.as_le_byte_array(): [u8; 16]`):
/// - `conn_id_0` contains bytes `[0 ..8 ]`.
/// - `conn_id_1` contains bytes `[8 ..16]`.
///
/// Again, note that `to_byte_array` uses LITTLE-ENDIAN order! This matches most host systems.
///
/// The `args` is a `BytesSource`, registered on the host side,
/// which can be read with `bytes_source_read`.
/// The contents of the buffer are the BSATN-encoding of the arguments to the reducer.
/// In the case of empty arguments, `args` will be 0, that is, invalid.
///
/// The `result_sink` is a `BytesSink`, registered on the host side,
/// which can be written to with `bytes_sink_write`.
/// Procedures are expected to always write to this sink
/// the BSATN-serialized bytes of a value of the procedure's return type.
///
/// Procedures always return the error 0. All other return values are reserved.
#[no_mangle]
extern "C" fn __call_procedure__(
    id: usize,
    sender_0: u64,
    sender_1: u64,
    sender_2: u64,
    sender_3: u64,
    conn_id_0: u64,
    conn_id_1: u64,
    timestamp: u64,
    args: BytesSource,
    result_sink: BytesSink,
) -> i16 {
    // Piece together `sender_i` into an `Identity`.
    let sender = reconstruct_sender_identity(sender_0, sender_1, sender_2, sender_3);

    // Piece together `conn_id_i` into a `ConnectionId`.
    let conn_id = reconstruct_connection_id(conn_id_0, conn_id_1);

    let timestamp = Timestamp::from_micros_since_unix_epoch(timestamp as i64);

    // Assemble the `ProcedureContext`.
    let ctx = ProcedureContext {
        connection_id: conn_id,
        sender,
        timestamp,
    };

    // Grab the list of procedures, which is populated by the preinit functions.
    let procedures = PROCEDURES.get().unwrap();

    // Deserialize the args and pass them to the actual procedure.
    let res = with_read_args(args, |args| procedures[id](ctx, args));

    // Write the result bytes to the `result_sink`.
    write_to_sink(result_sink, &res);

    // Return 0 for no error. Procedures always either trap or return 0.
    0
}

/// Called by the host to execute an anonymous view.
///
/// The `args` is a `BytesSource`, registered on the host side,
/// which can be read with `bytes_source_read`.
/// The contents of the buffer are the BSATN-encoding of the arguments to the view.
/// In the case of empty arguments, `args` will be 0, that is, invalid.
///
/// The output of the view is written to a `BytesSink`,
/// registered on the host side, with `bytes_sink_write`.
#[no_mangle]
extern "C" fn __call_view_anon__(id: usize, args: BytesSource, sink: BytesSink) -> i16 {
    let views = ANONYMOUS_VIEWS.get().unwrap();
    write_to_sink(
        sink,
        &with_read_args(args, |args| {
            views[id](AnonymousViewContext { db: LocalReadOnly {} }, args)
        }),
    );
    0
}

/// Called by the host to execute a view when the `sender` calls the view identified by `id` with `args`.
/// See [`__call_reducer__`] for more commentary on the arguments.
///
/// The `args` is a `BytesSource`, registered on the host side,
/// which can be read with `bytes_source_read`.
/// The contents of the buffer are the BSATN-encoding of the arguments to the view.
/// In the case of empty arguments, `args` will be 0, that is, invalid.
///
/// The output of the view is written to a `BytesSink`,
/// registered on the host side, with `bytes_sink_write`.
#[no_mangle]
extern "C" fn __call_view__(
    id: usize,
    sender_0: u64,
    sender_1: u64,
    sender_2: u64,
    sender_3: u64,
    args: BytesSource,
    sink: BytesSink,
) -> i16 {
    // Piece together `sender_i` into an `Identity`.
    let sender = [sender_0, sender_1, sender_2, sender_3];
    let sender: [u8; 32] = bytemuck::must_cast(sender);
    let sender = Identity::from_byte_array(sender); // The LITTLE-ENDIAN constructor.

    let views = VIEWS.get().unwrap();
    let db = LocalReadOnly {};

    write_to_sink(
        sink,
        &with_read_args(args, |args| views[id](ViewContext { sender, db }, args)),
    );
    0
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

/// Look up the jwt associated with `connection_id`.
pub fn get_jwt(connection_id: ConnectionId) -> Option<String> {
    let mut buf = IterBuf::take();
    let source = sys::get_jwt(connection_id.as_le_byte_array())?;
    if source == BytesSource::INVALID {
        return None;
    }
    read_bytes_source_into(source, &mut buf);
    Some(std::str::from_utf8(&buf).unwrap().to_string())
}

/// Read `source` from the host fully into `buf`.
fn read_bytes_source_into(source: BytesSource, buf: &mut Vec<u8>) {
    const INVALID: i16 = NO_SUCH_BYTES as i16;

    // For reducer arguments, the `buf` will almost certainly already be large enough,
    // as it comes from `IterBuf`, which start at 64KiB.
    // But reading the remaining length and calling `buf.reserve` is a negligible cost,
    // and in the future we may want to use this method to read other `BytesSource`s into other buffers.
    // I (pgoldman 2025-09-26) also value having it as an example of correct usage of `bytes_source_remaining_length`.
    let len = {
        let mut len = 0;
        let ret = unsafe { sys::raw::bytes_source_remaining_length(source, &raw mut len) };
        match ret {
            0 => len,
            INVALID => panic!("invalid source passed"),
            _ => unreachable!(),
        }
    };
    buf.reserve(buf.len().saturating_sub(len as usize));

    // Because we've reserved space in our buffer already, this loop should be unnecessary.
    // We expect the first call to `bytes_source_read` to always return `-1`.
    // I (pgoldman 2025-09-26) am leaving the loop here because there's no downside to it,
    // and in the future we may want to support `BytesSource`s which don't have a known length ahead of time
    // (i.e. put arbitrary streams in `BytesSource` on the host side rather than just `Bytes` buffers),
    // at which point the loop will become useful again.
    loop {
        // Write into the spare capacity of the buffer.
        let buf_ptr = buf.spare_capacity_mut();
        let spare_len = buf_ptr.len();
        let mut buf_len = buf_ptr.len();
        let buf_ptr = buf_ptr.as_mut_ptr().cast();
        let ret = unsafe { sys::raw::bytes_source_read(source, buf_ptr, &mut buf_len) };
        if ret <= 0 {
            // SAFETY: `bytes_source_read` just appended `buf_len` bytes to `buf`.
            unsafe { buf.set_len(buf.len() + buf_len) };
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
pub fn volatile_nonatomic_schedule_immediate<'de, A: Args<'de>, R: Reducer<'de, A>, R2: FnInfo<Invoke = ReducerFn>>(
    _reducer: R,
    args: A,
) {
    let arg_bytes = bsatn::to_vec(&SerDeArgs(args)).unwrap();

    // Schedule the reducer.
    sys::volatile_nonatomic_schedule_immediate(R2::NAME, &arg_bytes)
}
