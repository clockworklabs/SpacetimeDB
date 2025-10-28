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

pub(super) fn get_hook_function<'s>(
    scope: &mut PinScope<'s, '_>,
    hooks_obj: Local<'_, Object>,
    name: &'static StringConst,
) -> ExcResult<Local<'s, Function>> {
    let key = name.string(scope);
    let object = property(scope, hooks_obj, key)?;
    cast!(scope, object, Function, "module function hook `{}`", name.as_str()).map_err(|e| e.throw(scope))
}

pub(super) fn set_hook_slots(
    scope: &mut PinScope<'_, '_>,
    abi: AbiVersion,
    hooks: &[(ModuleHook, Local<'_, Function>)],
) -> ExcResult<()> {
    // Make sure to call `set_slot` first, as it creates the annex
    // and `set_embedder_data` is currently buggy.
    let ctx = scope.get_current_context();
    let hooks_info = HooksInfo::get_or_create(&ctx);
    for &(hook, func) in hooks {
        hooks_info
            .register(hook, abi)
            .map_err(|_| TypeError("cannot call register_hooks multiple times").throw(scope))?;
        ctx.set_embedder_data(hook.to_slot_index(), func.into());
    }
    Ok(())
}

#[derive(enum_map::Enum, Copy, Clone)]
pub(in crate::host::v8) enum ModuleHook {
    DescribeModule,
    CallReducer,
}

impl ModuleHook {
    /// Get the `v8::Context::{get,set}_embedder_data` slot that holds this hook.
    fn to_slot_index(self) -> i32 {
        match self {
            ModuleHook::DescribeModule => 20,
            ModuleHook::CallReducer => 21,
        }
    }
}

#[derive(Default)]
struct HooksInfo {
    abi: OnceCell<AbiVersion>,
    registered: EnumMap<ModuleHook, OnceCell<()>>,
}

impl HooksInfo {
    fn get_or_create(ctx: &Context) -> Rc<Self> {
        ctx.get_slot().unwrap_or_else(|| {
            let this = Rc::<Self>::default();
            ctx.set_slot(this.clone());
            this
        })
    }

    fn register(&self, hook: ModuleHook, abi: AbiVersion) -> Result<(), ()> {
        if *self.abi.get_or_init(|| abi) != abi {
            return Err(());
        }
        self.registered[hook].set(())
    }

    fn get(&self, hook: ModuleHook) -> Option<AbiVersion> {
        self.registered[hook].get().and(self.abi.get().copied())
    }
}

#[derive(Copy, Clone)]
pub(in crate::host::v8) struct HookFunction<'s>(pub AbiVersion, pub Local<'s, Function>);

/// Returns the hook function previously registered in [`register_hooks`].
pub(in crate::host::v8) fn get_hook<'scope>(
    scope: &mut PinScope<'scope, '_>,
    hook: ModuleHook,
) -> Option<HookFunction<'scope>> {
    let ctx = scope.get_current_context();
    let hooks = ctx.get_slot::<HooksInfo>()?;

    let abi_version = hooks.get(hook)?;

    let hooks = ctx
        .get_embedder_data(scope, hook.to_slot_index())
        .expect("if `AbiVersion` is set the hook must be set");
    Some(HookFunction(abi_version, hooks.cast()))
}
