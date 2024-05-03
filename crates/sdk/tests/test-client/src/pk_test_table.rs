use crate::module_bindings::*;
use anyhow::anyhow;
use spacetimedb_sdk::table::TableWithPrimaryKey;
use std::sync::Arc;
use test_counter::TestCounter;

pub trait PkTestTable: TableWithPrimaryKey {
    fn as_value(&self) -> i32;

    fn is_insert_reducer_event(event: &Self::ReducerEvent) -> bool;
    fn is_update_reducer_event(event: &Self::ReducerEvent) -> bool;
    fn is_delete_reducer_event(event: &Self::ReducerEvent) -> bool;

    fn insert(k: Self::PrimaryKey, v: i32);
    fn update(k: Self::PrimaryKey, v: i32);
    fn delete(k: Self::PrimaryKey);
}

pub fn insert_update_delete_one<T: PkTestTable>(
    test_counter: &Arc<TestCounter>,
    key: T::PrimaryKey,
    initial_value: i32,
    update_value: i32,
) where
    T::PrimaryKey: std::fmt::Debug + Send + 'static,
{
    let mut insert_result = Some(test_counter.add_test(format!("insert-{}", T::TABLE_NAME)));
    let mut update_result = Some(test_counter.add_test(format!("update-{}", T::TABLE_NAME)));
    let mut delete_result = Some(test_counter.add_test(format!("delete-{}", T::TABLE_NAME)));

    let mut on_delete = {
        let key_dup = key.clone();
        Some(move |row: &T, reducer_event: Option<&T::ReducerEvent>| {
            if delete_result.is_some() {
                let run_checks = || {
                    if row.primary_key() != &key_dup || row.as_value() != update_value {
                        anyhow::bail!(
                            "Unexpected row value. Expected ({:?}, {}) but found {:?}",
                            key_dup,
                            update_value,
                            row
                        );
                    }
                    reducer_event
                        .ok_or(anyhow!("Expected a reducer event, but found None."))
                        .map(T::is_delete_reducer_event)
                        .and_then(|is_good| is_good.then_some(()).ok_or(anyhow!("Unexpected ReducerEvent variant.")))?;
                    Ok(())
                };

                (delete_result.take().unwrap())(run_checks());
            }
        })
    };

    let mut on_update = {
        let key_dup = key.clone();
        Some(move |old: &T, new: &T, reducer_event: Option<&T::ReducerEvent>| {
            if update_result.is_some() {
                let run_checks = || {
                    if old.primary_key() != &key_dup || old.as_value() != initial_value {
                        anyhow::bail!(
                            "Unexpected old row value. Expected ({:?}, {}) but found {:?}",
                            key_dup,
                            initial_value,
                            old,
                        );
                    }
                    if new.primary_key() != &key_dup || new.as_value() != update_value {
                        anyhow::bail!(
                            "Unexpected new row value. Expected ({:?}, {}) but found {:?}",
                            key_dup,
                            update_value,
                            new,
                        );
                    }
                    reducer_event
                        .ok_or(anyhow!("Expected a reducer event, but found None."))
                        .map(T::is_update_reducer_event)
                        .and_then(|is_good| is_good.then_some(()).ok_or(anyhow!("Unexpected ReducerEvent variant.")))?;
                    Ok(())
                };

                (update_result.take().unwrap())(run_checks());

                T::on_delete(on_delete.take().unwrap());

                T::delete(key_dup.clone());
            }
        })
    };

    let key_dup = key.clone();

    T::on_insert(move |row, reducer_event| {
        if insert_result.is_some() {
            let run_checks = || {
                if row.primary_key() != &key_dup || row.as_value() != initial_value {
                    anyhow::bail!(
                        "Unexpected row value. Expected ({:?}, {}) but found {:?}",
                        key_dup,
                        initial_value,
                        row
                    );
                }
                reducer_event
                    .ok_or(anyhow!("Expected a reducer event, but found None."))
                    .map(T::is_insert_reducer_event)
                    .and_then(|is_good| is_good.then_some(()).ok_or(anyhow!("Unexpected ReducerEvent variant.")))?;

                Ok(())
            };

            (insert_result.take().unwrap())(run_checks());

            T::on_update(on_update.take().unwrap());

            T::update(key_dup.clone(), update_value);
        }
    });

    T::insert(key, initial_value);
}

macro_rules! impl_pk_test_table {
    ($table:ty {
        Key = $key:ty;
        key_field_name = $field_name:ident;
        insert_reducer = $insert_reducer:ident;
        insert_reducer_event = $insert_reducer_event:ident;
        delete_reducer = $delete_reducer:ident;
        delete_reducer_event = $delete_reducer_event:ident;
        update_reducer = $update_reducer:ident;
        update_reducer_event = $update_reducer_event:ident;
    }) => {
        impl PkTestTable for $table {
            fn as_value(&self) -> i32 {
                self.data
            }

            fn is_insert_reducer_event(event: &Self::ReducerEvent) -> bool {
                matches!(event, ReducerEvent::$insert_reducer_event(_))
            }
            fn is_delete_reducer_event(event: &Self::ReducerEvent) -> bool {
                matches!(event, ReducerEvent::$delete_reducer_event(_))
            }
            fn is_update_reducer_event(event: &Self::ReducerEvent) -> bool {
                matches!(event, ReducerEvent::$update_reducer_event(_))
            }

            fn insert(key: Self::PrimaryKey, value: i32) {
                $insert_reducer(key, value);
            }
            fn delete(key: Self::PrimaryKey) {
                $delete_reducer(key);
            }
            fn update(key: Self::PrimaryKey, new_value: i32) {
                $update_reducer(key, new_value);
            }
        }
    };
    ($($table:ty { $($stuff:tt)* })*) => {
        $(impl_pk_test_table!($table { $($stuff)* });)*
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
    }

    PkAddress {
        Key = Address;
        key_field_name = a;
        insert_reducer = insert_pk_address;
        insert_reducer_event = InsertPkAddress;
        delete_reducer = delete_pk_address;
        delete_reducer_event = DeletePkAddress;
        update_reducer = update_pk_address;
        update_reducer_event = UpdatePkAddress;
    }

}
