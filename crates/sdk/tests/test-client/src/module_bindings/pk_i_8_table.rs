// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::pk_i_8_type::PkI8;
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    lib as __lib, sats as __sats, ws_messages as __ws,
};

pub struct PkI8TableHandle<'ctx> {
    imp: __sdk::db_connection::TableHandle<PkI8>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
pub trait PkI8TableAccess {
    #[allow(non_snake_case)]
    fn pk_i_8(&self) -> PkI8TableHandle<'_>;
}

impl PkI8TableAccess for super::RemoteTables {
    fn pk_i_8(&self) -> PkI8TableHandle<'_> {
        PkI8TableHandle {
            imp: self.imp.get_table::<PkI8>("PkI8"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct PkI8InsertCallbackId(__sdk::callbacks::CallbackId);
pub struct PkI8DeleteCallbackId(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::Table for PkI8TableHandle<'ctx> {
    type Row = PkI8;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = PkI8> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = PkI8InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> PkI8InsertCallbackId {
        PkI8InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: PkI8InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = PkI8DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> PkI8DeleteCallbackId {
        PkI8DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: PkI8DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

pub struct PkI8UpdateCallbackId(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::TableWithPrimaryKey for PkI8TableHandle<'ctx> {
    type UpdateCallbackId = PkI8UpdateCallbackId;

    fn on_update(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row, &Self::Row) + Send + 'static,
    ) -> PkI8UpdateCallbackId {
        PkI8UpdateCallbackId(self.imp.on_update(Box::new(callback)))
    }

    fn remove_on_update(&self, callback: PkI8UpdateCallbackId) {
        self.imp.remove_on_update(callback.0)
    }
}

pub(super) fn parse_table_update(
    deletes: Vec<__ws::EncodedValue>,
    inserts: Vec<__ws::EncodedValue>,
) -> __anyhow::Result<__sdk::spacetime_module::TableUpdate<PkI8>> {
    __sdk::spacetime_module::TableUpdate::parse_table_update_with_primary_key::<i8>(deletes, inserts, |row: &PkI8| {
        &row.n
    })
    .context("Failed to parse table update for table \"PkI8\"")
}

pub struct PkI8NUnique<'ctx> {
    imp: __sdk::client_cache::UniqueConstraint<PkI8, i8>,
    phantom: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

impl<'ctx> PkI8TableHandle<'ctx> {
    pub fn n(&self) -> PkI8NUnique<'ctx> {
        PkI8NUnique {
            imp: self.imp.get_unique_constraint::<i8>("n", |row| &row.n),
            phantom: std::marker::PhantomData,
        }
    }
}

impl<'ctx> PkI8NUnique<'ctx> {
    pub fn find(&self, col_val: &i8) -> Option<PkI8> {
        self.imp.find(col_val)
    }
}