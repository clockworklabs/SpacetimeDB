use std::cell::OnceCell;
use std::rc::Rc;

use enum_map::EnumMap;
use v8::{Context, Function, Local, Object, PinScope};

use super::AbiVersion;
use crate::host::v8::de::property;
use crate::host::v8::error::ExcResult;
use crate::host::v8::error::Throwable;
use crate::host::v8::error::TypeError;
use crate::host::v8::from_value::cast;
use crate::host::v8::string::StringConst;

/// Returns the hook function `name` on `hooks_obj`.
pub(super) fn get_hook_function<'scope>(
    scope: &mut PinScope<'scope, '_>,
    hooks_obj: Local<'_, Object>,
    name: &'static StringConst,
) -> ExcResult<Local<'scope, Function>> {
    let key = name.string(scope);
    let object = property(scope, hooks_obj, key)?;
    cast!(scope, object, Function, "module function hook `{}`", name.as_str()).map_err(|e| e.throw(scope))
}

/// Returns the hook function `name` on `hooks_obj`.
pub(super) fn get_optional_hook_function<'scope>(
    scope: &mut PinScope<'scope, '_>,
    hooks_obj: Local<'_, Object>,
    name: &'static StringConst,
) -> ExcResult<Option<Local<'scope, Function>>> {
    let key = name.string(scope);
    let object = property(scope, hooks_obj, key)?;
    (!object.is_null_or_undefined())
        .then(|| cast!(scope, object, Function, "module function hook `{}`", name.as_str()))
        .transpose()
        .map_err(|e| e.throw(scope))
}

/// Registers all the module function `hooks`
/// and sets the given `AbiVersion` to `abi`.
pub(super) fn set_hook_slots(
    scope: &mut PinScope<'_, '_>,
    abi: AbiVersion,
    hooks: &[(ModuleHookKey, Local<'_, Function>)],
) -> ExcResult<()> {
    let ctx = scope.get_current_context();
    let hooks_info = HooksInfo::get_or_create(&ctx, abi)
        .map_err(|_| TypeError("cannot call `register_hooks` from different versions").throw(scope))?;
    for &(hook, func) in hooks {
        hooks_info
            .register(hook)
            .map_err(|_| TypeError("cannot call `register_hooks` multiple times").throw(scope))?;
        ctx.set_embedder_data(hook.to_slot_index(), func.into());
    }
    Ok(())
}

/// Registers the hooks for the current module instance so they can be reconstructed later
/// from procedure syscalls that only have access to the current V8 context.
pub(in super::super) fn set_registered_hooks(scope: &mut PinScope<'_, '_>, hooks: &HookFunctions<'_>) -> ExcResult<()> {
    let &HookFunctions {
        abi,
        recv,
        describe_module,
        get_error_constructor,
        sender_error_class,
        call_reducer,
        call_view,
        call_view_anon,
        call_procedure,
        call_http_handler,
        call_migration,
    } = hooks;
    let required = [
        (ModuleHookKey::DescribeModule, describe_module),
        (ModuleHookKey::CallReducer, call_reducer),
    ];
    let optional = [
        (ModuleHookKey::CallView, call_view),
        (ModuleHookKey::CallAnonymousView, call_view_anon),
        (ModuleHookKey::CallProcedure, call_procedure),
        (ModuleHookKey::CallHttpHandler, call_http_handler),
        (ModuleHookKey::CallMigration, call_migration),
        (ModuleHookKey::GetErrorConstructor, get_error_constructor),
        (ModuleHookKey::SenderErrorClass, sender_error_class),
    ];
    let to_register = required
        .into_iter()
        .chain(optional.into_iter().filter_map(|(key, hook)| Some((key, hook?))))
        .collect::<Vec<_>>();

    set_hook_slots(scope, abi, &to_register)?;

    let ctx = scope.get_current_context();
    ctx.set_embedder_data(RECV_SLOT_INDEX, recv);

    Ok(())
}

#[derive(enum_map::Enum, Copy, Clone)]
pub(in super::super) enum ModuleHookKey {
    DescribeModule,
    CallReducer,
    CallView,
    CallAnonymousView,
    CallProcedure,
    CallHttpHandler,
    CallMigration,
    GetErrorConstructor,
    SenderErrorClass,
}

impl ModuleHookKey {
    /// Returns the index for the slot that holds the module function hook.
    /// The index is passed to `v8::Context::{get,set}_embedder_data`.
    fn to_slot_index(self) -> i32 {
        self as i32
    }
}

/// Context embedder slot holding the receiver (`this`) value used for hook calls.
pub(super) const RECV_SLOT_INDEX: i32 = ModuleHookKey::SenderErrorClass as i32 + 1;

/// Holds the `AbiVersion` used by the module
/// and the module hooks registered by the module
/// for that version.
struct HooksInfo {
    abi: AbiVersion,
    registered: EnumMap<ModuleHookKey, OnceCell<()>>,
}

impl HooksInfo {
    /// Returns, and possibly creates, the [`HooksInfo`] stored in `ctx`.
    ///
    /// Returns an error if `abi` doesn't match the abi version in the
    /// already existing `HooksInfo`.
    fn get_or_create(ctx: &Context, abi: AbiVersion) -> Result<Rc<Self>, ()> {
        match ctx.get_slot::<Self>() {
            Some(this) if this.abi == abi => Ok(this),
            Some(_) => Err(()),
            None => {
                let this = Rc::new(Self {
                    abi,
                    registered: EnumMap::default(),
                });
                ctx.set_slot(this.clone());
                Ok(this)
            }
        }
    }

    /// Mark down the given `hook` as registered, returning an error if it already was.
    fn register(&self, hook: ModuleHookKey) -> Result<(), ()> {
        self.registered[hook].set(())
    }
}

#[derive(Copy, Clone)]
/// The actual callable module hook functions and their abi version.
pub(in super::super) struct HookFunctions<'scope> {
    pub abi: AbiVersion,
    /// The `this` variable to pass to the hook functions.
    pub recv: Local<'scope, v8::Value>,
    /// describe_module and call_reducer existed in v1.0, but everything else is `Option`al
    pub describe_module: Local<'scope, Function>,
    pub get_error_constructor: Option<Local<'scope, Function>>,
    pub sender_error_class: Option<Local<'scope, Function>>,
    pub call_reducer: Local<'scope, Function>,
    pub call_view: Option<Local<'scope, Function>>,
    pub call_view_anon: Option<Local<'scope, Function>>,
    pub call_procedure: Option<Local<'scope, Function>>,
    pub call_http_handler: Option<Local<'scope, Function>>,
    pub call_migration: Option<Local<'scope, Function>>,
}

/// Returns the hook function previously registered in [`register_hooks`].
pub(in super::super) fn get_registered_hooks<'scope>(
    scope: &mut PinScope<'scope, '_>,
) -> Option<HookFunctions<'scope>> {
    let ctx = scope.get_current_context();
    let hooks = ctx.get_slot::<HooksInfo>()?;

    let get = |hook: ModuleHookKey| {
        hooks.registered[hook].get().map(|()| {
            ctx.get_embedder_data(scope, hook.to_slot_index())
                .expect("if the hook is registered it must have been set")
                .cast()
        })
    };

    Some(HookFunctions {
        abi: hooks.abi,
        recv: ctx
            .get_embedder_data(scope, RECV_SLOT_INDEX)
            .unwrap_or_else(|| v8::undefined(scope).into()),
        describe_module: get(ModuleHookKey::DescribeModule)?,
        get_error_constructor: get(ModuleHookKey::GetErrorConstructor),
        sender_error_class: get(ModuleHookKey::SenderErrorClass),
        call_reducer: get(ModuleHookKey::CallReducer)?,
        call_view: get(ModuleHookKey::CallView),
        call_view_anon: get(ModuleHookKey::CallAnonymousView),
        call_procedure: get(ModuleHookKey::CallProcedure),
        call_http_handler: get(ModuleHookKey::CallHttpHandler),
        call_migration: get(ModuleHookKey::CallMigration),
    })
}
