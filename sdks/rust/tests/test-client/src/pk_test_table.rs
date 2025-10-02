use crate::module_bindings::*;
use spacetimedb_sdk::{i256, u256, ConnectionId, Event, Identity, Table, TableWithPrimaryKey};
use std::sync::Arc;
use test_counter::TestCounter;

pub trait PkTestTable: std::fmt::Debug {
    type PrimaryKey: Clone + Send + Sync + PartialEq + std::fmt::Debug + 'static;
    fn as_value(&self) -> i32;
    fn primary_key(&self) -> &Self::PrimaryKey;

    fn is_insert_reducer_event(event: &Reducer) -> bool;
    fn is_update_reducer_event(event: &Reducer) -> bool;
    fn is_delete_reducer_event(event: &Reducer) -> bool;

    fn insert(ctx: &impl RemoteDbContext, k: Self::PrimaryKey, v: i32);
    fn update(ctx: &impl RemoteDbContext, k: Self::PrimaryKey, v: i32);
    fn delete(ctx: &impl RemoteDbContext, k: Self::PrimaryKey);

    fn on_insert(ctx: &impl RemoteDbContext, callback: impl FnMut(&EventContext, &Self) + Send + 'static);
    fn on_delete(ctx: &impl RemoteDbContext, callback: impl FnMut(&EventContext, &Self) + Send + 'static);
    fn on_update(ctx: &impl RemoteDbContext, callback: impl FnMut(&EventContext, &Self, &Self) + Send + 'static);
}

pub fn insert_update_delete_one<T: PkTestTable>(
    ctx: &impl RemoteDbContext,
    test_counter: &Arc<TestCounter>,
    key: T::PrimaryKey,
    initial_value: i32,
    update_value: i32,
) {
    let mut insert_result = Some(test_counter.add_test(format!("insert-{}", std::any::type_name::<T>())));
    let mut update_result = Some(test_counter.add_test(format!("update-{}", std::any::type_name::<T>())));
    let mut delete_result = Some(test_counter.add_test(format!("delete-{}", std::any::type_name::<T>())));

    let mut on_delete = {
        let key_dup = key.clone();
        Some(move |ctx: &EventContext, row: &T| {
            if delete_result.is_some() {
                let run_checks = || {
                    if row.primary_key() != &key_dup || row.as_value() != update_value {
                        anyhow::bail!("Unexpected row value. Expected ({key_dup:?}, {update_value}) but found {row:?}");
                    }
                    let Event::Reducer(reducer_event) = &ctx.event else {
                        anyhow::bail!("Expected a reducer event");
                    };
                    anyhow::ensure!(
                        T::is_delete_reducer_event(&reducer_event.reducer),
                        "Unexpected Reducer variant {:?}",
                        reducer_event.reducer,
                    );
                    Ok(())
                };

                (delete_result.take().unwrap())(run_checks());
            }
        })
    };

    let mut on_update = {
        let key_dup = key.clone();
        Some(move |ctx: &EventContext, old: &T, new: &T| {
            if update_result.is_some() {
                let run_checks = || {
                    if old.primary_key() != &key_dup || old.as_value() != initial_value {
                        anyhow::bail!(
                            "Unexpected old row value. Expected ({key_dup:?}, {initial_value}) but found {old:?}",
                        );
                    }
                    if new.primary_key() != &key_dup || new.as_value() != update_value {
                        anyhow::bail!(
                            "Unexpected new row value. Expected ({key_dup:?}, {update_value}) but found {new:?}",
                        );
                    }
                    let Event::Reducer(reducer_event) = &ctx.event else {
                        anyhow::bail!("Expected a reducer event");
                    };
                    anyhow::ensure!(
                        T::is_update_reducer_event(&reducer_event.reducer),
                        "Unexpected Reducer variant {:?}",
                        reducer_event.reducer,
                    );
                    Ok(())
                };

                (update_result.take().unwrap())(run_checks());

                T::on_delete(ctx, on_delete.take().unwrap());

                T::delete(ctx, key_dup.clone());
            }
        })
    };

    let key_dup = key.clone();

    T::on_insert(ctx, move |ctx, row| {
        if insert_result.is_some() {
            let run_checks = || {
                if row.primary_key() != &key_dup || row.as_value() != initial_value {
                    anyhow::bail!("Unexpected row value. Expected ({key_dup:?}, {initial_value}) but found {row:?}");
                }
                let Event::Reducer(reducer_event) = &ctx.event else {
                    anyhow::bail!("Expected a reducer event");
                };
                anyhow::ensure!(
                    T::is_insert_reducer_event(&reducer_event.reducer),
                    "Unexpected Reducer variant {:?}",
                    reducer_event.reducer,
                );
                Ok(())
            };

            (insert_result.take().unwrap())(run_checks());

            T::on_update(ctx, on_update.take().unwrap());

            T::update(ctx, key_dup.clone(), update_value);
        }
    });

    T::insert(ctx, key, initial_value);
}

macro_rules! impl_pk_test_table {
    (__impl $table:ident {
        Key = $key:ty;
        key_field_name = $field_name:ident;
        insert_reducer = $insert_reducer:ident;
        insert_reducer_event = $insert_reducer_event:ident;
        delete_reducer = $delete_reducer:ident;
        delete_reducer_event = $delete_reducer_event:ident;
        update_reducer = $update_reducer:ident;
        update_reducer_event = $update_reducer_event:ident;
        accessor_method = $accessor_method:ident;
    }) => {
        impl PkTestTable for $table {
            type PrimaryKey = $key;

            fn as_value(&self) -> i32 {
                self.data
            }

            fn primary_key(&self) -> &Self::PrimaryKey {
                &self.$field_name
            }

            fn is_insert_reducer_event(event: &Reducer) -> bool {
                matches!(event, Reducer::$insert_reducer_event { .. })
            }
            fn is_delete_reducer_event(event: &Reducer) -> bool {
                matches!(event, Reducer::$delete_reducer_event { .. })
            }
            fn is_update_reducer_event(event: &Reducer) -> bool {
                matches!(event, Reducer::$update_reducer_event { .. })
            }

            fn insert(ctx: &impl RemoteDbContext, key: Self::PrimaryKey, value: i32) {
                ctx.reducers().$insert_reducer(key, value).unwrap();
            }
            fn delete(ctx: &impl RemoteDbContext, key: Self::PrimaryKey) {
                ctx.reducers().$delete_reducer(key).unwrap();
            }
            fn update(ctx: &impl RemoteDbContext, key: Self::PrimaryKey, new_value: i32) {
                ctx.reducers().$update_reducer(key, new_value).unwrap();
            }

            fn on_insert(ctx: &impl RemoteDbContext, callback: impl FnMut(&EventContext, &Self) + Send + 'static) {
                ctx.db().$accessor_method().on_insert(callback);
            }
            fn on_delete(ctx: &impl RemoteDbContext, callback: impl FnMut(&EventContext, &Self) + Send + 'static) {
                ctx.db().$accessor_method().on_delete(callback);
            }
            fn on_update(ctx: &impl RemoteDbContext, callback: impl FnMut(&EventContext, &Self, &Self) + Send + 'static) {
                ctx.db().$accessor_method().on_update(callback);
            }

        }
    };
    ($($table:ident { $($stuff:tt)* })*) => {
        $(impl_pk_test_table!(__impl $table { $($stuff)* });)*
    };
}

impl_pk_test_table! {
    PkU8 {
        Key = u8;
        key_field_name = n;
        insert_reducer = insert_pk_u_8;
        insert_reducer_event = InsertPkU8;
        delete_reducer = delete_pk_u_8;
        delete_reducer_event = DeletePkU8;
        update_reducer = update_pk_u_8;
        update_reducer_event = UpdatePkU8;
        accessor_method = pk_u_8;
    }
    PkU16 {
        Key = u16;
        key_field_name = n;
        insert_reducer = insert_pk_u_16;
        insert_reducer_event = InsertPkU16;
        delete_reducer = delete_pk_u_16;
        delete_reducer_event = DeletePkU16;
        update_reducer = update_pk_u_16;
        update_reducer_event = UpdatePkU16;
        accessor_method = pk_u_16;
    }
    PkU32 {
        Key = u32;
        key_field_name = n;
        insert_reducer = insert_pk_u_32;
        insert_reducer_event = InsertPkU32;
        delete_reducer = delete_pk_u_32;
        delete_reducer_event = DeletePkU32;
        update_reducer = update_pk_u_32;
        update_reducer_event = UpdatePkU32;
        accessor_method = pk_u_32;
    }
    PkU32Two {
        Key = u32;
        key_field_name = n;
        insert_reducer = insert_pk_u_32_two;
        insert_reducer_event = InsertPkU32Two;
        delete_reducer = delete_pk_u_32_two;
        delete_reducer_event = DeletePkU32Two;
        update_reducer = update_pk_u_32_two;
        update_reducer_event = UpdatePkU32Two;
        accessor_method = pk_u_32_two;
    }
    PkU64 {
        Key = u64;
        key_field_name = n;
        insert_reducer = insert_pk_u_64;
        insert_reducer_event = InsertPkU64;
        delete_reducer = delete_pk_u_64;
        delete_reducer_event = DeletePkU64;
        update_reducer = update_pk_u_64;
        update_reducer_event = UpdatePkU64;
        accessor_method = pk_u_64;
    }
    PkU128 {
        Key = u128;
        key_field_name = n;
        insert_reducer = insert_pk_u_128;
        insert_reducer_event = InsertPkU128;
        delete_reducer = delete_pk_u_128;
        delete_reducer_event = DeletePkU128;
        update_reducer = update_pk_u_128;
        update_reducer_event = UpdatePkU128;
        accessor_method = pk_u_128;
    }
    PkU256 {
        Key = u256;
        key_field_name = n;
        insert_reducer = insert_pk_u_256;
        insert_reducer_event = InsertPkU256;
        delete_reducer = delete_pk_u_256;
        delete_reducer_event = DeletePkU256;
        update_reducer = update_pk_u_256;
        update_reducer_event = UpdatePkU256;
        accessor_method = pk_u_256;
    }

    PkI8 {
        Key = i8;
        key_field_name = n;
        insert_reducer = insert_pk_i_8;
        insert_reducer_event = InsertPkI8;
        delete_reducer = delete_pk_i_8;
        delete_reducer_event = DeletePkI8;
        update_reducer = update_pk_i_8;
        update_reducer_event = UpdatePkI8;
        accessor_method = pk_i_8;
    }
    PkI16 {
        Key = i16;
        key_field_name = n;
        insert_reducer = insert_pk_i_16;
        insert_reducer_event = InsertPkI16;
        delete_reducer = delete_pk_i_16;
        delete_reducer_event = DeletePkI16;
        update_reducer = update_pk_i_16;
        update_reducer_event = UpdatePkI16;
        accessor_method = pk_i_16;
    }
    PkI32 {
        Key = i32;
        key_field_name = n;
        insert_reducer = insert_pk_i_32;
        insert_reducer_event = InsertPkI32;
        delete_reducer = delete_pk_i_32;
        delete_reducer_event = DeletePkI32;
        update_reducer = update_pk_i_32;
        update_reducer_event = UpdatePkI32;
        accessor_method = pk_i_32;
    }
    PkI64 {
        Key = i64;
        key_field_name = n;
        insert_reducer = insert_pk_i_64;
        insert_reducer_event = InsertPkI64;
        delete_reducer = delete_pk_i_64;
        delete_reducer_event = DeletePkI64;
        update_reducer = update_pk_i_64;
        update_reducer_event = UpdatePkI64;
        accessor_method = pk_i_64;
    }
    PkI128 {
        Key = i128;
        key_field_name = n;
        insert_reducer = insert_pk_i_128;
        insert_reducer_event = InsertPkI128;
        delete_reducer = delete_pk_i_128;
        delete_reducer_event = DeletePkI128;
        update_reducer = update_pk_i_128;
        update_reducer_event = UpdatePkI128;
        accessor_method = pk_i_128;
    }
    PkI256 {
        Key = i256;
        key_field_name = n;
        insert_reducer = insert_pk_i_256;
        insert_reducer_event = InsertPkI256;
        delete_reducer = delete_pk_i_256;
        delete_reducer_event = DeletePkI256;
        update_reducer = update_pk_i_256;
        update_reducer_event = UpdatePkI256;
        accessor_method = pk_i_256;
    }

    PkBool {
        Key = bool;
        key_field_name = b;
        insert_reducer = insert_pk_bool;
        insert_reducer_event = InsertPkBool;
        delete_reducer = delete_pk_bool;
        delete_reducer_event = DeletePkBool;
        update_reducer = update_pk_bool;
        update_reducer_event = UpdatePkBool;
        accessor_method = pk_bool;
    }

    PkString {
        Key = String;
        key_field_name = s;
        insert_reducer = insert_pk_string;
        insert_reducer_event = InsertPkString;
        delete_reducer = delete_pk_string;
        delete_reducer_event = DeletePkString;
        update_reducer = update_pk_string;
        update_reducer_event = UpdatePkString;
        accessor_method = pk_string;
    }

    PkIdentity {
        Key = Identity;
        key_field_name = i;
        insert_reducer = insert_pk_identity;
        insert_reducer_event = InsertPkIdentity;
        delete_reducer = delete_pk_identity;
        delete_reducer_event = DeletePkIdentity;
        update_reducer = update_pk_identity;
        update_reducer_event = UpdatePkIdentity;
        accessor_method = pk_identity;
    }

    PkConnectionId {
        Key = ConnectionId;
        key_field_name = a;
        insert_reducer = insert_pk_connection_id;
        insert_reducer_event = InsertPkConnectionId;
        delete_reducer = delete_pk_connection_id;
        delete_reducer_event = DeletePkConnectionId;
        update_reducer = update_pk_connection_id;
        update_reducer_event = UpdatePkConnectionId;
        accessor_method = pk_connection_id;
    }

}
