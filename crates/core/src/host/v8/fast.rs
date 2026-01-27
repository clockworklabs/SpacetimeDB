use std::os::raw::c_void;

use v8::fast_api::{CFunction, CFunctionInfo, CTypeInfo, FastApiCallbackOptions, Int64Representation, Type as CType};

/// # Safety
/// `FUNCTION_PTR` combined with the `CFunctionInfo` must be a valid `CFunction`
pub(super) unsafe trait IntoFastCall<M> {
    const RETURN_TYPE: CTypeInfo;
    const PARAMS: &[CTypeInfo];
    const FUNCTION_PTR: *const c_void;
}

pub(super) const fn into_cfunc<F: IntoFastCall<M>, M>(f: F) -> CFunction {
    _into_cfunc::<F, M, false>(f)
}

pub(super) const fn into_cfunc_bigint64<F: IntoFastCall<M>, M>(f: F) -> CFunction {
    _into_cfunc::<F, M, true>(f)
}

pub(super) const fn _into_cfunc<F: IntoFastCall<M>, M, const BIGINT64: bool>(f: F) -> CFunction {
    std::mem::forget(f);
    let type_info: &'static CFunctionInfo = const {
        &CFunctionInfo::new(
            F::RETURN_TYPE,
            F::PARAMS,
            if BIGINT64 {
                Int64Representation::BigInt
            } else {
                Int64Representation::Number
            },
        )
    };
    CFunction::new(F::FUNCTION_PTR, type_info)
}

// Noa wrote this for rustpython
const fn zst_ref_out_of_thin_air<T: 'static>() -> &'static T {
    const {
        if std::mem::size_of::<T>() != 0 {
            panic!("can't use a non-zero-sized type here")
        }
        // SAFETY: we just confirmed that T is zero-sized, so we can
        //         pull a value of it out of thin air.
        unsafe { std::ptr::NonNull::<T>::dangling().as_ref() }
    }
}

trait FastType {
    type Static;
    type Refd<'a>;
    const TYPE_INFO: CTypeInfo;
}

macro_rules! impl_fast_type {
    ($t:ty, $cty:ident) => {
        impl FastType for $t {
            type Static = Self;
            type Refd<'a> = Self;
            const TYPE_INFO: CTypeInfo = CType::as_info(CType::$cty);
        }
    };
}

impl_fast_type!((), Void);
impl_fast_type!(bool, Bool);
impl_fast_type!(u8, Uint8);
impl_fast_type!(i32, Int32);
impl_fast_type!(u32, Uint32);
impl_fast_type!(i64, Int64);
impl_fast_type!(u64, Uint64);
impl_fast_type!(f32, Float32);
impl_fast_type!(f64, Float64);
impl FastType for v8::Local<'_, v8::Value> {
    type Static = v8::Value;
    type Refd<'a> = Self;
    const TYPE_INFO: CTypeInfo = CType::V8Value.as_info();
}

macro_rules! impl_fast_call {
    ($($args:ident),*) => {
        // unsafe impl<Ret: FastType, $($args: FastType),*> IntoFastCall<(fn($($args::Static),*) -> Ret,)>
        //     for extern "C" fn(v8::Local<'_, v8::Value>, $($args),*) -> Ret
        // {
        //     const RETURN_TYPE: CTypeInfo = Ret::TYPE_INFO;
        //     const PARAMS: &[CTypeInfo] = &[$($args::TYPE_INFO),*];
        // }
        unsafe impl<Func, Ret: FastType, $($args: FastType),*> IntoFastCall<(fn($($args),*) -> Ret, FastApiCallbackOptions<'static>)>
            for Func
        where
            Func: 'static
                + Fn(v8::Local<'_, v8::Value>, $($args::Refd<'_>,)* &mut FastApiCallbackOptions<'_>) -> Ret
                + Fn(v8::Local<'_, v8::Value>, $($args,)* &mut FastApiCallbackOptions<'_>) -> Ret
        {
            const RETURN_TYPE: CTypeInfo = Ret::TYPE_INFO;
            const PARAMS: &[CTypeInfo] = &[CType::V8Value.as_info(), $($args::TYPE_INFO,)* CType::CallbackOptions.as_info()];
            const FUNCTION_PTR: *const c_void = {
                #[allow(non_snake_case)]
                extern "C" fn cfunc<Func, Ret: FastType, $($args: FastType),*>(
                    recv: v8::Local<'_, v8::Value>,
                    $($args: $args::Refd<'_>,)*
                    options: &mut FastApiCallbackOptions<'_>
                ) -> Ret
                where
                    Func: 'static
                        + Fn(v8::Local<'_, v8::Value>, $($args::Refd<'_>,)* &mut FastApiCallbackOptions<'_>) -> Ret
                {
                    zst_ref_out_of_thin_air::<Func>()(recv, $($args,)* options)
                }
                cfunc::<Func, Ret, $($args),*> as *const c_void
            };
        }
    };
}

impl_fast_call!();
impl_fast_call!(A);
impl_fast_call!(A, B);
impl_fast_call!(A, B, C);
impl_fast_call!(A, B, C, D);
impl_fast_call!(A, B, C, D, E);
impl_fast_call!(A, B, C, D, E, F);
