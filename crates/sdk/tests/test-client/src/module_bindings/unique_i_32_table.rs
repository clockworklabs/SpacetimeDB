// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::unique_i_32_type::UniqueI32;
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    lib as __lib, sats as __sats, ws_messages as __ws,
};

pub struct UniqueI32TableHandle<'ctx> {
    imp: __sdk::db_connection::TableHandle<UniqueI32>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
pub trait UniqueI32TableAccess {
    #[allow(non_snake_case)]
    fn unique_i_32(&self) -> UniqueI32TableHandle<'_>;
}

impl UniqueI32TableAccess for super::RemoteTables {
    fn unique_i_32(&self) -> UniqueI32TableHandle<'_> {
        UniqueI32TableHandle {
            imp: self.imp.get_table::<UniqueI32>("UniqueI32"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct UniqueI32InsertCallbackId(__sdk::callbacks::CallbackId);
pub struct UniqueI32DeleteCallbackId(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::Table for UniqueI32TableHandle<'ctx> {
    type Row = UniqueI32;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = UniqueI32> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = UniqueI32InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> UniqueI32InsertCallbackId {
        UniqueI32InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: UniqueI32InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = UniqueI32DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> UniqueI32DeleteCallbackId {
        UniqueI32DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: UniqueI32DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

pub(super) fn parse_table_update(
    deletes: Vec<__ws::EncodedValue>,
    inserts: Vec<__ws::EncodedValue>,
) -> __anyhow::Result<__sdk::spacetime_module::TableUpdate<UniqueI32>> {
    __sdk::spacetime_module::TableUpdate::parse_table_update_no_primary_key(deletes, inserts)
        .context("Failed to parse table update for table \"UniqueI32\"")
}

pub struct UniqueI32NUnique<'ctx> {
    imp: __sdk::client_cache::UniqueConstraint<UniqueI32, i32>,
    phantom: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

impl<'ctx> UniqueI32TableHandle<'ctx> {
    pub fn n(&self) -> UniqueI32NUnique<'ctx> {
        UniqueI32NUnique {
            imp: self.imp.get_unique_constraint::<i32>("n", |row| &row.n),
            phantom: std::marker::PhantomData,
        }
    }
}

impl<'ctx> UniqueI32NUnique<'ctx> {
    pub fn find(&self, col_val: &i32) -> Option<UniqueI32> {
        self.imp.find(col_val)
    }
}