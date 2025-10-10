use crate::module_bindings::*;
use spacetimedb_sdk::{i256, u256, ConnectionId, Event, Identity, Table};
use std::sync::Arc;
use test_counter::TestCounter;

pub trait UniqueTestTable: std::fmt::Debug {
    type Key: Clone + Send + Sync + PartialEq + std::fmt::Debug + 'static;

    fn as_key(&self) -> &Self::Key;
    fn as_value(&self) -> i32;

    fn is_insert_reducer_event(event: &Reducer) -> bool;
    fn is_delete_reducer_event(event: &Reducer) -> bool;

    fn insert(ctx: &impl RemoteDbContext, k: Self::Key, v: i32);
    fn delete(ctx: &impl RemoteDbContext, k: Self::Key);

    fn on_insert(ctx: &impl RemoteDbContext, callback: impl FnMut(&EventContext, &Self) + Send + 'static);
    fn on_delete(ctx: &impl RemoteDbContext, callback: impl FnMut(&EventContext, &Self) + Send + 'static);
}

pub fn insert_then_delete_one<T: UniqueTestTable>(
    ctx: &impl RemoteDbContext,
    test_counter: &Arc<TestCounter>,
    key: T::Key,
    value: i32,
) {
    let mut insert_result = Some(test_counter.add_test(format!("insert-{}", std::any::type_name::<T>())));
    let mut delete_result = Some(test_counter.add_test(format!("delete-{}", std::any::type_name::<T>())));

    let mut on_delete = {
        let key_dup = key.clone();
        Some(move |ctx: &EventContext, row: &T| {
            if delete_result.is_some() {
                let run_checks = || {
                    if row.as_key() != &key_dup || row.as_value() != value {
                        anyhow::bail!("Unexpected row value. Expected ({key_dup:?}, {value}) but found {row:?}");
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

    let key_dup = key.clone();

    T::on_insert(ctx, move |ctx, row| {
        if insert_result.is_some() {
            let run_checks = || {
                if row.as_key() != &key_dup || row.as_value() != value {
                    anyhow::bail!("Unexpected row value. Expected ({key_dup:?}, {value}) but found {row:?}");
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

            T::on_delete(ctx, on_delete.take().unwrap());

            T::delete(ctx, key_dup.clone());
        }
    });

    T::insert(ctx, key, value);
}

macro_rules! impl_unique_test_table {
    (__impl $table:ident {
        Key = $key:ty;
        key_field_name = $field_name:ident;
        insert_reducer = $insert_reducer:ident;
        insert_reducer_event = $insert_reducer_event:ident;
        delete_reducer = $delete_reducer:ident;
        delete_reducer_event = $delete_reducer_event:ident;
        accessor_method = $accessor_method:ident;
    }) => {
        impl UniqueTestTable for $table {
            type Key = $key;

            fn as_key(&self) -> &Self::Key {
                &self.$field_name
            }
            fn as_value(&self) -> i32 {
                self.data
            }

            fn is_insert_reducer_event(event: &Reducer) -> bool {
                matches!(event, Reducer::$insert_reducer_event { .. })
            }
            fn is_delete_reducer_event(event: &Reducer) -> bool {
                matches!(event, Reducer::$delete_reducer_event { .. })
            }

            fn insert(ctx: &impl RemoteDbContext, key: Self::Key, value: i32) {
                ctx.reducers().$insert_reducer(key, value).unwrap();
            }
            fn delete(ctx: &impl RemoteDbContext, key: Self::Key) {
                ctx.reducers().$delete_reducer(key).unwrap();
            }

            fn on_insert(ctx: &impl RemoteDbContext, callback: impl FnMut(&EventContext, &$table) + Send + 'static) {
                ctx.db().$accessor_method().on_insert(callback);
            }
            fn on_delete(ctx: &impl RemoteDbContext, callback: impl FnMut(&EventContext, &$table) + Send + 'static) {
                ctx.db().$accessor_method().on_delete(callback);
            }
        }
    };
    ($($table:ident { $($stuff:tt)* })*) => {
        $(impl_unique_test_table!(__impl $table { $($stuff)* });)*
    };
}

impl_unique_test_table! {
    UniqueU8 {
        Key = u8;
        key_field_name = n;
        insert_reducer = insert_unique_u_8;
        insert_reducer_event = InsertUniqueU8;
        delete_reducer = delete_unique_u_8;
        delete_reducer_event = DeleteUniqueU8;
        accessor_method = unique_u_8;
    }
    UniqueU16 {
        Key = u16;
        key_field_name = n;
        insert_reducer = insert_unique_u_16;
        insert_reducer_event = InsertUniqueU16;
        delete_reducer = delete_unique_u_16;
        delete_reducer_event = DeleteUniqueU16;
        accessor_method = unique_u_16;
    }
    UniqueU32 {
        Key = u32;
        key_field_name = n;
        insert_reducer = insert_unique_u_32;
        insert_reducer_event = InsertUniqueU32;
        delete_reducer = delete_unique_u_32;
        delete_reducer_event = DeleteUniqueU32;
        accessor_method = unique_u_32;
    }
    UniqueU64 {
        Key = u64;
        key_field_name = n;
        insert_reducer = insert_unique_u_64;
        insert_reducer_event = InsertUniqueU64;
        delete_reducer = delete_unique_u_64;
        delete_reducer_event = DeleteUniqueU64;
        accessor_method = unique_u_64;
    }
    UniqueU128 {
        Key = u128;
        key_field_name = n;
        insert_reducer = insert_unique_u_128;
        insert_reducer_event = InsertUniqueU128;
        delete_reducer = delete_unique_u_128;
        delete_reducer_event = DeleteUniqueU128;
        accessor_method = unique_u_128;
    }
    UniqueU256 {
        Key = u256;
        key_field_name = n;
        insert_reducer = insert_unique_u_256;
        insert_reducer_event = InsertUniqueU256;
        delete_reducer = delete_unique_u_256;
        delete_reducer_event = DeleteUniqueU256;
        accessor_method = unique_u_256;
    }

    UniqueI8 {
        Key = i8;
        key_field_name = n;
        insert_reducer = insert_unique_i_8;
        insert_reducer_event = InsertUniqueI8;
        delete_reducer = delete_unique_i_8;
        delete_reducer_event = DeleteUniqueI8;
        accessor_method = unique_i_8;
    }
    UniqueI16 {
        Key = i16;
        key_field_name = n;
        insert_reducer = insert_unique_i_16;
        insert_reducer_event = InsertUniqueI16;
        delete_reducer = delete_unique_i_16;
        delete_reducer_event = DeleteUniqueI16;
        accessor_method = unique_i_16;
    }
    UniqueI32 {
        Key = i32;
        key_field_name = n;
        insert_reducer = insert_unique_i_32;
        insert_reducer_event = InsertUniqueI32;
        delete_reducer = delete_unique_i_32;
        delete_reducer_event = DeleteUniqueI32;
        accessor_method = unique_i_32;
    }
    UniqueI64 {
        Key = i64;
        key_field_name = n;
        insert_reducer = insert_unique_i_64;
        insert_reducer_event = InsertUniqueI64;
        delete_reducer = delete_unique_i_64;
        delete_reducer_event = DeleteUniqueI64;
        accessor_method = unique_i_64;
    }
    UniqueI128 {
        Key = i128;
        key_field_name = n;
        insert_reducer = insert_unique_i_128;
        insert_reducer_event = InsertUniqueI128;
        delete_reducer = delete_unique_i_128;
        delete_reducer_event = DeleteUniqueI128;
        accessor_method = unique_i_128;
    }
    UniqueI256 {
        Key = i256;
        key_field_name = n;
        insert_reducer = insert_unique_i_256;
        insert_reducer_event = InsertUniqueI256;
        delete_reducer = delete_unique_i_256;
        delete_reducer_event = DeleteUniqueI256;
        accessor_method = unique_i_256;
    }

    UniqueBool {
        Key = bool;
        key_field_name = b;
        insert_reducer = insert_unique_bool;
        insert_reducer_event = InsertUniqueBool;
        delete_reducer = delete_unique_bool;
        delete_reducer_event = DeleteUniqueBool;
        accessor_method = unique_bool;
    }

    UniqueString {
        Key = String;
        key_field_name = s;
        insert_reducer = insert_unique_string;
        insert_reducer_event = InsertUniqueString;
        delete_reducer = delete_unique_string;
        delete_reducer_event = DeleteUniqueString;
        accessor_method = unique_string;
    }

    UniqueIdentity {
        Key = Identity;
        key_field_name = i;
        insert_reducer = insert_unique_identity;
        insert_reducer_event = InsertUniqueIdentity;
        delete_reducer = delete_unique_identity;
        delete_reducer_event = DeleteUniqueIdentity;
        accessor_method = unique_identity;
    }

    UniqueConnectionId {
        Key = ConnectionId;
        key_field_name = a;
        insert_reducer = insert_unique_connection_id;
        insert_reducer_event = InsertUniqueConnectionId;
        delete_reducer = delete_unique_connection_id;
        delete_reducer_event = DeleteUniqueConnectionId;
        accessor_method = unique_connection_id;
    }
}
