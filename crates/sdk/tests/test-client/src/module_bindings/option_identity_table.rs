// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN YOUR MODULE SOURCE CODE INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{
	self as __sdk,
	__lib,
	__sats,
	__ws,
};
use super::option_identity_type::OptionIdentity;

/// Table handle for the table `option_identity`.
///
/// Obtain a handle from the [`OptionIdentityTableAccess::option_identity`] method on [`super::RemoteTables`],
/// like `ctx.db.option_identity()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.option_identity().on_insert(...)`.
pub struct OptionIdentityTableHandle<'ctx> {
    imp: __sdk::TableHandle<OptionIdentity>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `option_identity`.
///
/// Implemented for [`super::RemoteTables`].
pub trait OptionIdentityTableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`OptionIdentityTableHandle`], which mediates access to the table `option_identity`.
    fn option_identity(&self) -> OptionIdentityTableHandle<'_>;
}

impl OptionIdentityTableAccess for super::RemoteTables {
    fn option_identity(&self) -> OptionIdentityTableHandle<'_> {
        OptionIdentityTableHandle {
            imp: self.imp.get_table::<OptionIdentity>("option_identity"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct OptionIdentityInsertCallbackId(__sdk::CallbackId);
pub struct OptionIdentityDeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for OptionIdentityTableHandle<'ctx> {
    type Row = OptionIdentity;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 { self.imp.count() }
    fn iter(&self) -> impl Iterator<Item = OptionIdentity> + '_ { self.imp.iter() }

    type InsertCallbackId = OptionIdentityInsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> OptionIdentityInsertCallbackId {
        OptionIdentityInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: OptionIdentityInsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = OptionIdentityDeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> OptionIdentityDeleteCallbackId {
        OptionIdentityDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: OptionIdentityDeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {

        let _table = client_cache.get_or_make_table::<OptionIdentity>("option_identity");
}

#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __sdk::Result<__sdk::TableUpdate<OptionIdentity>> {
    __sdk::TableUpdate::parse_table_update(raw_updates).map_err(|e| {
        __sdk::InternalError::failed_parse(
            "TableUpdate<OptionIdentity>",
            "TableUpdate",
        ).with_cause(e).into()
    })
}
