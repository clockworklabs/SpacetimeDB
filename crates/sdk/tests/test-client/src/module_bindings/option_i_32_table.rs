// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::option_i_32_type::OptionI32;
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    lib as __lib, sats as __sats, ws_messages as __ws,
};

/// Table handle for the table `option_i32`.
///
/// Obtain a handle from the [`OptionI32TableAccess::option_i_32`] method on [`super::RemoteTables`],
/// like `ctx.db.option_i_32()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.option_i_32().on_insert(...)`.
pub struct OptionI32TableHandle<'ctx> {
    imp: __sdk::db_connection::TableHandle<OptionI32>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `option_i32`.
///
/// Implemented for [`super::RemoteTables`].
pub trait OptionI32TableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`OptionI32TableHandle`], which mediates access to the table `option_i32`.
    fn option_i_32(&self) -> OptionI32TableHandle<'_>;
}

impl OptionI32TableAccess for super::RemoteTables {
    fn option_i_32(&self) -> OptionI32TableHandle<'_> {
        OptionI32TableHandle {
            imp: self.imp.get_table::<OptionI32>(26),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct OptionI32InsertCallbackId(__sdk::callbacks::CallbackId);
pub struct OptionI32DeleteCallbackId(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::Table for OptionI32TableHandle<'ctx> {
    type Row = OptionI32;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = OptionI32> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = OptionI32InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> OptionI32InsertCallbackId {
        OptionI32InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: OptionI32InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = OptionI32DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> OptionI32DeleteCallbackId {
        OptionI32DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: OptionI32DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __anyhow::Result<__sdk::spacetime_module::TableUpdate<OptionI32>> {
    __sdk::spacetime_module::TableUpdate::parse_table_update_no_primary_key(raw_updates)
        .context("Failed to parse table update for table \"option_i32\"")
}
