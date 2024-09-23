// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::vec_i_64_type::VecI64;
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    lib as __lib, sats as __sats, ws_messages as __ws,
};

pub struct VecI64TableHandle<'ctx> {
    imp: __sdk::db_connection::TableHandle<VecI64>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
pub trait VecI64TableAccess {
    #[allow(non_snake_case)]
    fn vec_i_64(&self) -> VecI64TableHandle<'_>;
}

impl VecI64TableAccess for super::RemoteTables {
    fn vec_i_64(&self) -> VecI64TableHandle<'_> {
        VecI64TableHandle {
            imp: self.imp.get_table::<VecI64>("VecI64"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct VecI64InsertCallbackId(__sdk::callbacks::CallbackId);
pub struct VecI64DeleteCallbackId(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::Table for VecI64TableHandle<'ctx> {
    type Row = VecI64;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = VecI64> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = VecI64InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecI64InsertCallbackId {
        VecI64InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: VecI64InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = VecI64DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecI64DeleteCallbackId {
        VecI64DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: VecI64DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

pub(super) fn parse_table_update(
    deletes: Vec<__ws::EncodedValue>,
    inserts: Vec<__ws::EncodedValue>,
) -> __anyhow::Result<__sdk::spacetime_module::TableUpdate<VecI64>> {
    __sdk::spacetime_module::TableUpdate::parse_table_update_no_primary_key(deletes, inserts)
        .context("Failed to parse table update for table \"VecI64\"")
}