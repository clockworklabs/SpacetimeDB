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

/// Registers all the module function `hooks`
/// and sets the given `AbiVersion` to `abi`.
pub(super) fn set_hook_slots(
    scope: &mut PinScope<'_, '_>,
    abi: AbiVersion,
    hooks: &[(ModuleHookKey, Local<'_, Function>)],
) -> ExcResult<()> {
    // Make sure to call `set_slot` first, as it creates the annex
    // and `set_embedder_data` is currently buggy.
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

#[derive(enum_map::Enum, Copy, Clone)]
pub(in super::super) enum ModuleHookKey {
    DescribeModule,
    CallReducer,
}

impl ModuleHookKey {
    /// Returns the index for the slot that holds the module function hook.
    /// The index is passed to `v8::Context::{get,set}_embedder_data`.
    fn to_slot_index(self) -> i32 {
        match self {
            // high numbers to avoid overlapping with rusty_v8 - can be
            // reverted to just 0, 1... once denoland/rusty_v8#1868 merges
            ModuleHookKey::DescribeModule => 20,
            ModuleHookKey::CallReducer => 21,
        }
    }
}

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

    /// Returns the `AbiVersion` for the given `hook`, if any.
    fn get(&self, hook: ModuleHookKey) -> Option<AbiVersion> {
        self.registered[hook].get().map(|_| self.abi)
    }
}

#[derive(Copy, Clone)]
/// The actual callable module hook function and its abi version.
pub(in super::super) struct HookFunction<'scope>(pub AbiVersion, pub Local<'scope, Function>);

/// Returns the hook function previously registered in [`register_hooks`].
pub(in super::super) fn get_hook<'scope>(
    scope: &mut PinScope<'scope, '_>,
    hook: ModuleHookKey,
) -> Option<HookFunction<'scope>> {
    let ctx = scope.get_current_context();
    let hooks = ctx.get_slot::<HooksInfo>()?;

    let abi_version = hooks.get(hook)?;

    let hooks = ctx
        .get_embedder_data(scope, hook.to_slot_index())
        .expect("if `AbiVersion` is set the hook must be set");
    Some(HookFunction(abi_version, hooks.cast()))
}
