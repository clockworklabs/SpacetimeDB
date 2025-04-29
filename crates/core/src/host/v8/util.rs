use std::mem::MaybeUninit;

pub(super) struct StringConst(v8::OneByteConst);

impl StringConst {
    pub(super) const fn new(s: &'static str) -> Self {
        Self(v8::String::create_external_onebyte_const(s.as_bytes()))
    }
    pub(super) fn string<'s>(&'static self, scope: &mut v8::HandleScope<'s, ()>) -> v8::Local<'s, v8::String> {
        // unwrap() b/c create_external_onebyte_const asserts new_from_onebyte_const's
        // preconditions (str < kMaxLength)
        v8::String::new_from_onebyte_const(scope, &self.0).unwrap()
    }
}

pub(super) fn scratch_buf<const N: usize>() -> [MaybeUninit<u8>; N] {
    [const { MaybeUninit::uninit() }; N]
}

pub(super) trait ObjectExt {
    fn get_str<'s>(
        &self,
        scope: &mut v8::HandleScope<'s>,
        key: &'static StringConst,
    ) -> Option<v8::Local<'s, v8::Value>>;
}

impl ObjectExt for v8::Object {
    fn get_str<'s>(
        &self,
        scope: &mut v8::HandleScope<'s>,
        key: &'static StringConst,
    ) -> Option<v8::Local<'s, v8::Value>> {
        let key = key.string(scope);
        self.get(scope, key.into())
    }
}

pub(super) fn iter_array<'a, 'b, 's, T, F>(
    scope: &'a mut v8::HandleScope<'s>,
    array: v8::Local<'b, v8::Array>,
    mut map: F,
) -> impl Iterator<Item = ExcResult<T>> + use<'a, 'b, 's, T, F>
where
    F: FnMut(&mut v8::HandleScope<'s>, v8::Local<'s, v8::Value>) -> ExcResult<T> + 'a,
{
    (0..array.length()).map(move |i| {
        let val = array.get_index(scope, i).err()?;
        map(scope, val)
    })
}

#[derive(Debug)]
pub(super) struct ExceptionThrown;

#[derive(Debug)]
pub(super) enum ErrorOrException<Exc = ExceptionThrown> {
    Err(anyhow::Error),
    Exception(Exc),
}

impl<Exc> From<anyhow::Error> for ErrorOrException<Exc> {
    fn from(err: anyhow::Error) -> Self {
        Self::Err(err)
    }
}

impl From<ExceptionThrown> for ErrorOrException {
    fn from(err: ExceptionThrown) -> Self {
        Self::Exception(err)
    }
}

impl From<ErrorOrException<super::JsError>> for anyhow::Error {
    fn from(err: ErrorOrException<super::JsError>) -> Self {
        match err {
            ErrorOrException::Err(e) => e,
            ErrorOrException::Exception(e) => e.into(),
        }
    }
}

pub(super) trait ExceptionOptionExt {
    type T;
    fn err(self) -> Result<Self::T, ExceptionThrown>;
}
impl<T> ExceptionOptionExt for Option<T> {
    type T = T;
    fn err(self) -> Result<Self::T, ExceptionThrown> {
        self.ok_or(ExceptionThrown)
    }
}

pub(super) fn throw<'s, T, E>(scope: &mut v8::HandleScope<'s>, err: E) -> Result<T, ExceptionThrown>
where
    E: IntoException<'s>,
{
    let exc = err.into_exception(scope);
    scope.throw_exception(exc);
    Err(ExceptionThrown)
}

pub(super) trait ThrowExceptionResultExt<'s> {
    type T;
    fn throw(self, scope: &mut v8::HandleScope<'s>) -> Result<Self::T, ExceptionThrown>;
}

impl<'s, T, E: IntoException<'s>> ThrowExceptionResultExt<'s> for Result<T, E> {
    type T = T;
    fn throw(self, scope: &mut v8::HandleScope<'s>) -> Result<Self::T, ExceptionThrown> {
        self.or_else(|err| throw(scope, err))
    }
}

pub(super) trait IntoException<'s> {
    fn into_exception(self, scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::Value>;
}

impl<'s> IntoException<'s> for v8::Local<'s, v8::Value> {
    fn into_exception(self, _scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::Value> {
        self
    }
}

#[derive(Copy, Clone)]
pub struct TypeError<M>(pub M);

impl<'s, M: IntoJsString<'s>> IntoException<'s> for TypeError<M> {
    fn into_exception(self, scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::Value> {
        let msg = self.0.into_string(scope);
        v8::Exception::type_error(scope, msg)
    }
}

pub(super) trait IntoJsString<'s> {
    fn into_string(self, scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::String>;
}
impl<'s> IntoJsString<'s> for v8::Local<'s, v8::String> {
    fn into_string(self, _scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::String> {
        self
    }
}
impl<'s> IntoJsString<'s> for String {
    fn into_string(self, scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::String> {
        v8::String::new(scope, &self).unwrap()
    }
}
impl<'s> IntoJsString<'s> for &'static StringConst {
    fn into_string(self, scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::String> {
        self.string(scope)
    }
}

pub(super) fn nicer_callback<F, R>(f: F) -> v8::FunctionCallback
where
    F: Fn(&mut v8::HandleScope<'_>, v8::FunctionCallbackArguments<'_>) -> ExcResult<R> + Copy,
    R: ReturnValue,
{
    let cb = move |scope: &mut v8::HandleScope<'_>,
                   args: v8::FunctionCallbackArguments<'_>,
                   rv: v8::ReturnValue<'_>| {
        match f(scope, args) {
            Ok(value) => value.set_return_value(rv),
            Err(ExceptionThrown) => {}
        }
    };
    v8::MapFnTo::map_fn_to(cb)
}

pub(super) type ExcResult<T> = Result<T, ExceptionThrown>;

pub(super) trait ReturnValue {
    fn set_return_value(self, rv: v8::ReturnValue<'_>);
}

macro_rules! impl_return_value {
    ($t:ty, $self:ident, $func:ident($($args:tt)*)) => {
        impl ReturnValue for $t {
            fn set_return_value($self, mut rv: v8::ReturnValue<'_>) {
                rv.$func($($args)*);
            }
        }
    };
    ($t:ty, $func:ident) => {
        impl_return_value!($t, self, $func(self));
    };
}

impl_return_value!(v8::Local<'_, v8::Value>, set);
impl_return_value!(bool, set_bool);
impl_return_value!(i32, set_int32);
impl_return_value!(u32, set_uint32);
impl_return_value!(f64, set_double);
impl_return_value!((), self, set_undefined());

pub(super) fn external_synthetic_steps<'s, F>(f: F) -> v8::ExternalReference<'s>
where
    F: v8::MapFnTo<v8::SyntheticModuleEvaluationSteps<'s>>,
{
    let pointer = f.map_fn_to() as _;
    v8::ExternalReference { pointer }
}

macro_rules! ascii_str {
    ($str:expr) => {
        const { &$crate::host::v8::util::StringConst::new($str) }
    };
}
pub(super) use ascii_str;

macro_rules! strings {
    ($vis:vis $($name:ident = $val:expr),*$(,)?) => {
        $($vis static $name: $crate::host::v8::util::StringConst = $crate::host::v8::util::StringConst::new($val);)*
    };
}
pub(super) use strings;

macro_rules! module {
    ($name:ident = $module_name:expr, $($export_kind:ident($export_name:ident $($export:tt)*)),*$(,)?) => {
        mod $name {
            pub const SPEC: &str = $module_name;
            $crate::host::v8::util::strings!(pub SPEC_STRING = SPEC);

            #[allow(non_snake_case, non_upper_case_globals)]
            mod names {
                $crate::host::v8::util::strings!(pub(super) $($export_name = stringify!($export_name),)*);
            }

            pub fn make<'s>(scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::Module> {
                let export_names = [$(names::$export_name.string(scope),)*];
                let spec = SPEC_STRING.string(scope);
                v8::Module::create_synthetic_module(scope, spec, &export_names, evaluation_steps)
            }

            fn evaluation_steps<'s>(context: v8::Local<'s, v8::Context>, module: v8::Local<'s, v8::Module>) -> Option<v8::Local<'s, v8::Value>> {
                let scope = &mut *unsafe { v8::CallbackScope::new(context) };
                $({
                    let name = names::$export_name.string(scope);
                    let val =
                        $crate::host::v8::util::module!(@export scope, name, $export_kind($export_name $($export)*));
                    module.set_synthetic_module_export(scope, name, val)?;
                })*
                Some(v8::undefined(scope).into())
            }

            pub fn external_refs<'s>() -> impl Iterator<Item = v8::ExternalReference<'s>> {
                [
                    $crate::host::v8::util::external_synthetic_steps(evaluation_steps),
                    $($crate::host::v8::util::module!(@export_ref $export_kind($export_name $($export)*)),)*
                ].into_iter()
            }

            $($crate::host::v8::util::module!(@export_rust $export_kind($export_name $($export)*));)*
        }
    };
    (@export $scope:ident, $name:ident, function($export_name:ident)) => {{
        let func = v8::Function::new_raw($scope, $crate::host::v8::util::nicer_callback(super::$export_name)).unwrap();
        func.set_name($name);
        func.into()
    }};
    (@export_ref function($export_name:ident)) => {
        v8::ExternalReference { function: $crate::host::v8::util::nicer_callback(super::$export_name) }
    };
    (@export_rust function($($t:tt)*)) => {};
    (@export $scope:ident, $name:ident, symbol($export_name:ident = $symbol:expr)) => {{
        $export_name($scope).into()
    }};
    (@export_ref symbol($($t:tt)*)) => {
        #[cfg(any())] ()
    };
    (@export_rust symbol($export_name:ident = $symbol:expr)) => {
        pub fn $export_name<'s>(scope: &mut v8::HandleScope<'s, ()>) -> v8::Local<'s, v8::Symbol> {
            $crate::host::v8::util::strings!(STRING = $symbol);
            let string = STRING.string(scope);
            v8::Symbol::for_api(scope, string)
        }
    };
}
pub(super) use module;
