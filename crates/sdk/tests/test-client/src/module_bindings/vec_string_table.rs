// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::vec_string_type::VecString;
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    lib as __lib, sats as __sats, ws_messages as __ws,
};

pub struct VecStringTableHandle<'ctx> {
    imp: __sdk::db_connection::TableHandle<VecString>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
pub trait VecStringTableAccess {
    #[allow(non_snake_case)]
    fn vec_string(&self) -> VecStringTableHandle<'_>;
}

impl VecStringTableAccess for super::RemoteTables {
    fn vec_string(&self) -> VecStringTableHandle<'_> {
        VecStringTableHandle {
            imp: self.imp.get_table::<VecString>("VecString"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct VecStringInsertCallbackId(__sdk::callbacks::CallbackId);
pub struct VecStringDeleteCallbackId(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::Table for VecStringTableHandle<'ctx> {
    type Row = VecString;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = VecString> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = VecStringInsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecStringInsertCallbackId {
        VecStringInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: VecStringInsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = VecStringDeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecStringDeleteCallbackId {
        VecStringDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: VecStringDeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

pub(super) fn parse_table_update(
    deletes: Vec<__ws::EncodedValue>,
    inserts: Vec<__ws::EncodedValue>,
) -> __anyhow::Result<__sdk::spacetime_module::TableUpdate<VecString>> {
    __sdk::spacetime_module::TableUpdate::parse_table_update_no_primary_key(deletes, inserts)
        .context("Failed to parse table update for table \"VecString\"")
}