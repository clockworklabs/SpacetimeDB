use super::de::v8_interned_string;
use core::cell::RefCell;
use std::rc::Rc;
use v8::{Global, HandleScope, Local};

/// Returns a `KeyCache` for the current `scope`.
///
/// Creates the cache in the scope if it doesn't exist yet.
pub(super) fn get_or_create_key_cache(scope: &mut HandleScope<'_>) -> Rc<RefCell<KeyCache>> {
    let context = scope.get_current_context();
    context.get_slot::<RefCell<KeyCache>>().unwrap_or_else(|| {
        let cache = Rc::default();
        context.set_slot(Rc::clone(&cache));
        cache
    })
}

/// A cache for frequently used strings to avoid re-interning them.
#[derive(Default)]
pub(super) struct KeyCache {
    /// The `tag` property for sum values in JS.
    tag: Option<Global<v8::String>>,
    /// The `value` property for sum values in JS.
    value: Option<Global<v8::String>>,
}

impl KeyCache {
    /// Returns the `tag` property name.
    pub(super) fn tag<'scope>(&mut self, scope: &mut HandleScope<'scope>) -> Local<'scope, v8::String> {
        Self::get_or_create_key(scope, &mut self.tag, "tag")
    }

    /// Returns the `value` property name.
    pub(super) fn value<'scope>(&mut self, scope: &mut HandleScope<'scope>) -> Local<'scope, v8::String> {
        Self::get_or_create_key(scope, &mut self.value, "value")
    }

    /// Returns an interned string corresponding to `string`
    /// and memoizes the creation on the v8 side.
    fn get_or_create_key<'scope>(
        scope: &mut HandleScope<'scope>,
        slot: &mut Option<Global<v8::String>>,
        string: &str,
    ) -> Local<'scope, v8::String> {
        if let Some(s) = &*slot {
            v8::Local::new(scope, s)
        } else {
            let s = v8_interned_string(scope, string);
            *slot = Some(v8::Global::new(scope, s));
            s
        }
    }
}
