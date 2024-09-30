// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::disconnected_type::Disconnected;
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    lib as __lib, sats as __sats, ws_messages as __ws,
};

/// Table handle for the table `disconnected`.
///
/// Obtain a handle from the [`DisconnectedTableAccess::disconnected`] method on [`super::RemoteTables`],
/// like `ctx.db.disconnected()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.disconnected().on_insert(...)`.
pub struct DisconnectedTableHandle<'ctx> {
    imp: __sdk::db_connection::TableHandle<Disconnected>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `disconnected`.
///
/// Implemented for [`super::RemoteTables`].
pub trait DisconnectedTableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`DisconnectedTableHandle`], which mediates access to the table `disconnected`.
    fn disconnected(&self) -> DisconnectedTableHandle<'_>;
}

impl DisconnectedTableAccess for super::RemoteTables {
    fn disconnected(&self) -> DisconnectedTableHandle<'_> {
        DisconnectedTableHandle {
            imp: self.imp.get_table::<Disconnected>("disconnected"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct DisconnectedInsertCallbackId(__sdk::callbacks::CallbackId);
pub struct DisconnectedDeleteCallbackId(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::Table for DisconnectedTableHandle<'ctx> {
    type Row = Disconnected;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = Disconnected> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = DisconnectedInsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> DisconnectedInsertCallbackId {
        DisconnectedInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: DisconnectedInsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = DisconnectedDeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> DisconnectedDeleteCallbackId {
        DisconnectedDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: DisconnectedDeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn parse_table_update(
    deletes: Vec<__ws::EncodedValue>,
    inserts: Vec<__ws::EncodedValue>,
) -> __anyhow::Result<__sdk::spacetime_module::TableUpdate<Disconnected>> {
    __sdk::spacetime_module::TableUpdate::parse_table_update_no_primary_key(deletes, inserts)
        .context("Failed to parse table update for table \"disconnected\"")
}
