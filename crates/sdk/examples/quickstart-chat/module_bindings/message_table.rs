use super::message_type::Message;
use anyhow::Context;
use spacetimedb_sdk::{
    callbacks::CallbackId, client_cache::TableCache, db_connection::TableHandle, spacetime_module::TableUpdate,
    table::Table, ws_messages as ws,
};
use std::marker::PhantomData;
use std::sync::Arc;

pub struct MessageTableHandle<'ctx> {
    imp: TableHandle<Message>,
    ctx: PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
pub trait message {
    fn message(&self) -> MessageTableHandle<'_>;
}

impl message for super::RemoteTables {
    fn message(&self) -> MessageTableHandle<'_> {
        MessageTableHandle {
            imp: self.imp.get_table::<Message>("message"),
            ctx: PhantomData,
        }
    }
}

pub struct MessageInsertCallbackId(CallbackId);
pub struct MessageDeleteCallbackId(CallbackId);

impl<'ctx> Table for MessageTableHandle<'ctx> {
    type Row = Message;

    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }

    fn iter(&self) -> impl Iterator<Item = Self::Row> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = MessageInsertCallbackId;
    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + Sync + 'static,
    ) -> Self::InsertCallbackId {
        MessageInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }
    fn remove_on_insert(&self, callback: Self::InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = MessageDeleteCallbackId;
    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + Sync + 'static,
    ) -> Self::DeleteCallbackId {
        MessageDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }
    fn remove_on_delete(&self, callback: Self::DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

impl<'ctx> MessageTableHandle<'ctx> {
    pub fn count(&self) -> u64 {
        self.imp.count()
    }

    pub fn iter(&self) -> impl Iterator<Item = Message> + '_ {
        self.imp.iter()
    }

    pub fn on_insert(
        &self,
        callback: impl FnMut(&super::EventContext, &Message) + Send + Sync + 'static,
    ) -> MessageInsertCallbackId {
        MessageInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }
    pub fn remove_on_insert(&self, callback: MessageInsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    pub fn on_delete(
        &self,
        callback: impl FnMut(&super::EventContext, &Message) + Send + Sync + 'static,
    ) -> MessageDeleteCallbackId {
        MessageDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }
    pub fn remove_on_delete(&self, callback: MessageDeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

pub(super) fn parse_table_update(
    deletes: Vec<ws::EncodedValue>,
    inserts: Vec<ws::EncodedValue>,
) -> spacetimedb_sdk::anyhow::Result<TableUpdate<Message>> {
    TableUpdate::parse_table_update_no_primary_key(deletes, inserts)
        .with_context(|| format!("Failed to parse table update for table {:?}", "message"))
}
