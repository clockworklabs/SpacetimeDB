// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::message_type::Message;
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    spacetimedb_lib as __lib, ws_messages as __ws,
};

pub struct MessageTableHandle<'ctx> {
    imp: __sdk::db_connection::TableHandle<Message>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
pub trait message {
    fn message(&self) -> MessageTableHandle<'_>;
}

impl message for super::RemoteTables {
    fn message(&self) -> MessageTableHandle<'_> {
        MessageTableHandle {
            imp: self.imp.get_table::<Message>("message"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct MessageInsertCallbackId(__sdk::callbacks::CallbackId);
pub struct MessageDeleteCallbackId(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::Table for MessageTableHandle<'ctx> {
    type Row = Message;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = Message> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = MessageInsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> MessageInsertCallbackId {
        MessageInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: MessageInsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = MessageDeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> MessageDeleteCallbackId {
        MessageDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: MessageDeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

pub(super) fn parse_table_update(
    deletes: Vec<__ws::EncodedValue>,
    inserts: Vec<__ws::EncodedValue>,
) -> __anyhow::Result<__sdk::spacetime_module::TableUpdate<Message>> {
    __sdk::spacetime_module::TableUpdate::parse_table_update_no_primary_key(deletes, inserts)
        .context("Failed to parse table update for table \"message\"")
}