// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::pk_bool_type::PkBool;
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    lib as __lib, sats as __sats, ws_messages as __ws,
};

pub struct PkBoolTableHandle<'ctx> {
    imp: __sdk::db_connection::TableHandle<PkBool>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
pub trait PkBoolTableAccess {
    #[allow(non_snake_case)]
    fn pk_bool(&self) -> PkBoolTableHandle<'_>;
}

impl PkBoolTableAccess for super::RemoteTables {
    fn pk_bool(&self) -> PkBoolTableHandle<'_> {
        PkBoolTableHandle {
            imp: self.imp.get_table::<PkBool>("PkBool"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct PkBoolInsertCallbackId(__sdk::callbacks::CallbackId);
pub struct PkBoolDeleteCallbackId(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::Table for PkBoolTableHandle<'ctx> {
    type Row = PkBool;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = PkBool> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = PkBoolInsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> PkBoolInsertCallbackId {
        PkBoolInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: PkBoolInsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = PkBoolDeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> PkBoolDeleteCallbackId {
        PkBoolDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: PkBoolDeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

pub struct PkBoolUpdateCallbackId(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::TableWithPrimaryKey for PkBoolTableHandle<'ctx> {
    type UpdateCallbackId = PkBoolUpdateCallbackId;

    fn on_update(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row, &Self::Row) + Send + 'static,
    ) -> PkBoolUpdateCallbackId {
        PkBoolUpdateCallbackId(self.imp.on_update(Box::new(callback)))
    }

    fn remove_on_update(&self, callback: PkBoolUpdateCallbackId) {
        self.imp.remove_on_update(callback.0)
    }
}

pub(super) fn parse_table_update(
    deletes: Vec<__ws::EncodedValue>,
    inserts: Vec<__ws::EncodedValue>,
) -> __anyhow::Result<__sdk::spacetime_module::TableUpdate<PkBool>> {
    __sdk::spacetime_module::TableUpdate::parse_table_update_with_primary_key::<bool>(
        deletes,
        inserts,
        |row: &PkBool| &row.b,
    )
    .context("Failed to parse table update for table \"PkBool\"")
}

pub struct PkBoolBUnique<'ctx> {
    imp: __sdk::client_cache::UniqueConstraint<PkBool, bool>,
    phantom: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

impl<'ctx> PkBoolTableHandle<'ctx> {
    pub fn b(&self) -> PkBoolBUnique<'ctx> {
        PkBoolBUnique {
            imp: self.imp.get_unique_constraint::<bool>("b", |row| &row.b),
            phantom: std::marker::PhantomData,
        }
    }
}

impl<'ctx> PkBoolBUnique<'ctx> {
    pub fn find(&self, col_val: &bool) -> Option<PkBool> {
        self.imp.find(col_val)
    }
}