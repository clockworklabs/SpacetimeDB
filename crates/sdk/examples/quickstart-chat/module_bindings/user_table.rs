use super::user_type::User;
use spacetimedb_sdk::{
    anyhow::Context, callbacks::CallbackId, client_cache::UniqueConstraint, db_connection::TableHandle,
    spacetime_module::TableUpdate, ws_messages as ws, Identity, Table, TableWithPrimaryKey,
};
use std::marker::PhantomData;
use std::sync::Arc;

pub struct UserTableHandle<'ctx> {
    imp: TableHandle<User>,
    ctx: PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
pub trait user {
    fn user(&self) -> UserTableHandle<'_>;
}

impl user for super::RemoteTables {
    fn user(&self) -> UserTableHandle<'_> {
        UserTableHandle {
            imp: self.imp.get_table::<User>("User"),
            ctx: PhantomData,
        }
    }
}

pub struct UserInsertCallbackId(CallbackId);
pub struct UserDeleteCallbackId(CallbackId);

impl<'ctx> Table for UserTableHandle<'ctx> {
    type Row = User;

    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }

    fn iter(&self) -> impl Iterator<Item = Self::Row> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = UserInsertCallbackId;
    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + Sync + 'static,
    ) -> Self::InsertCallbackId {
        UserInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }
    fn remove_on_insert(&self, callback: Self::InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = UserDeleteCallbackId;
    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + Sync + 'static,
    ) -> Self::DeleteCallbackId {
        UserDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }
    fn remove_on_delete(&self, callback: Self::DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

impl<'ctx> UserTableHandle<'ctx> {
    pub fn count(&self) -> u64 {
        self.imp.count()
    }

    pub fn iter(&self) -> impl Iterator<Item = User> + '_ {
        self.imp.iter()
    }

    pub fn on_insert(
        &self,
        callback: impl FnMut(&super::EventContext, &User) + Send + Sync + 'static,
    ) -> UserInsertCallbackId {
        UserInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }
    pub fn remove_on_insert(&self, callback: UserInsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    pub fn on_delete(
        &self,
        callback: impl FnMut(&super::EventContext, &User) + Send + Sync + 'static,
    ) -> UserDeleteCallbackId {
        UserDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }
    pub fn remove_on_delete(&self, callback: UserDeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

pub struct UserUpdateCallbackId(CallbackId);

impl<'ctx> TableWithPrimaryKey for UserTableHandle<'ctx> {
    type UpdateCallbackId = UserUpdateCallbackId;
    fn on_update(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row, &Self::Row) + Send + Sync + 'static,
    ) -> Self::UpdateCallbackId {
        UserUpdateCallbackId(self.imp.on_update(Box::new(callback)))
    }
    fn remove_on_update(&self, callback: Self::UpdateCallbackId) {
        self.imp.remove_on_update(callback.0)
    }
}

impl<'ctx> UserTableHandle<'ctx> {
    pub fn on_update(
        &self,
        callback: impl FnMut(&super::EventContext, &User, &User) + Send + Sync + 'static,
    ) -> UserUpdateCallbackId {
        UserUpdateCallbackId(self.imp.on_update(Box::new(callback)))
    }
    pub fn remove_on_update(&self, callback: UserUpdateCallbackId) {
        self.imp.remove_on_update(callback.0)
    }
}

pub struct UserIdentityUnique<'ctx> {
    imp: UniqueConstraint<User, Identity>,
    phantom: PhantomData<&'ctx super::RemoteTables>,
}

impl<'ctx> UserTableHandle<'ctx> {
    pub fn identity(&self) -> UserIdentityUnique<'ctx> {
        UserIdentityUnique {
            imp: self
                .imp
                .get_unique_constraint::<Identity>("identity", |user| &user.identity),
            phantom: PhantomData,
        }
    }
}

impl<'ctx> UserIdentityUnique<'ctx> {
    pub fn find(&self, col_val: Identity) -> Option<User> {
        self.imp.find(col_val)
    }
}

pub(super) fn parse_table_update(
    deletes: Vec<ws::EncodedValue>,
    inserts: Vec<ws::EncodedValue>,
) -> spacetimedb_sdk::anyhow::Result<TableUpdate<User>> {
    TableUpdate::parse_table_update_with_primary_key::<Identity>(deletes, inserts, |user: &User| &user.identity)
        .with_context(|| format!("Failed to parse table update for table {:?}", "User"))
}
