use crate::module_bindings::*;
use anyhow::anyhow;
use spacetimedb_sdk::{identity::Identity, table::TableType, Address};
use std::sync::Arc;
use test_counter::TestCounter;

pub trait UniqueTestTable: TableType {
    type Key: Clone + Send + Sync + PartialEq + std::fmt::Debug + 'static;

    fn as_key(&self) -> &Self::Key;
    fn as_value(&self) -> i32;

    fn is_insert_reducer_event(event: &Self::ReducerEvent) -> bool;
    fn is_delete_reducer_event(event: &Self::ReducerEvent) -> bool;

    fn insert(k: Self::Key, v: i32);
    fn delete(k: Self::Key);
}

pub fn insert_then_delete_one<T: UniqueTestTable>(test_counter: &Arc<TestCounter>, key: T::Key, value: i32) {
    let mut insert_result = Some(test_counter.add_test(format!("insert-{}", T::TABLE_NAME)));
    let mut delete_result = Some(test_counter.add_test(format!("delete-{}", T::TABLE_NAME)));

    let mut on_delete = {
        let key_dup = key.clone();
        Some(move |row: &T, reducer_event: Option<&T::ReducerEvent>| {
            if delete_result.is_some() {
                let run_checks = || {
                    if row.as_key() != &key_dup || row.as_value() != value {
                        anyhow::bail!(
                            "Unexpected row value. Expected ({:?}, {}) but found {:?}",
                            key_dup,
                            value,
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

    let key_dup = key.clone();

    T::on_insert(move |row, reducer_event| {
        if insert_result.is_some() {
            let run_checks = || {
                if row.as_key() != &key_dup || row.as_value() != value {
                    anyhow::bail!(
                        "Unexpected row value. Expected ({:?}, {}) but found {:?}",
                        key_dup,
                        value,
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

            T::on_delete(on_delete.take().unwrap());

            T::delete(key_dup.clone());
        }
    });

    T::insert(key, value);
}

macro_rules! impl_unique_test_table {
    ($table:ty {
        Key = $key:ty;
        key_field_name = $field_name:ident;
        insert_reducer = $insert_reducer:ident;
        insert_reducer_event = $insert_reducer_event:ident;
        delete_reducer = $delete_reducer:ident;
        delete_reducer_event = $delete_reducer_event:ident;
    }) => {
        impl UniqueTestTable for $table {
            type Key = $key;

            fn as_key(&self) -> &Self::Key {
                &self.$field_name
            }
            fn as_value(&self) -> i32 {
                self.data
            }

            fn is_insert_reducer_event(event: &Self::ReducerEvent) -> bool {
                matches!(event, ReducerEvent::$insert_reducer_event(_))
            }
            fn is_delete_reducer_event(event: &Self::ReducerEvent) -> bool {
                matches!(event, ReducerEvent::$delete_reducer_event(_))
            }

            fn insert(key: Self::Key, value: i32) {
                $insert_reducer(key, value);
            }
            fn delete(key: Self::Key) {
                $delete_reducer(key);
            }
        }
    };
    ($($table:ty { $($stuff:tt)* })*) => {
        $(impl_unique_test_table!($table { $($stuff)* });)*
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
    }
    UniqueU16 {
        Key = u16;
        key_field_name = n;
        insert_reducer = insert_unique_u_16;
        insert_reducer_event = InsertUniqueU16;
        delete_reducer = delete_unique_u_16;
        delete_reducer_event = DeleteUniqueU16;
    }
    UniqueU32 {
        Key = u32;
        key_field_name = n;
        insert_reducer = insert_unique_u_32;
        insert_reducer_event = InsertUniqueU32;
        delete_reducer = delete_unique_u_32;
        delete_reducer_event = DeleteUniqueU32;
    }
    UniqueU64 {
        Key = u64;
        key_field_name = n;
        insert_reducer = insert_unique_u_64;
        insert_reducer_event = InsertUniqueU64;
        delete_reducer = delete_unique_u_64;
        delete_reducer_event = DeleteUniqueU64;
    }
    UniqueU128 {
        Key = u128;
        key_field_name = n;
        insert_reducer = insert_unique_u_128;
        insert_reducer_event = InsertUniqueU128;
        delete_reducer = delete_unique_u_128;
        delete_reducer_event = DeleteUniqueU128;
    }

    UniqueI8 {
        Key = i8;
        key_field_name = n;
        insert_reducer = insert_unique_i_8;
        insert_reducer_event = InsertUniqueI8;
        delete_reducer = delete_unique_i_8;
        delete_reducer_event = DeleteUniqueI8;
    }
    UniqueI16 {
        Key = i16;
        key_field_name = n;
        insert_reducer = insert_unique_i_16;
        insert_reducer_event = InsertUniqueI16;
        delete_reducer = delete_unique_i_16;
        delete_reducer_event = DeleteUniqueI16;
    }
    UniqueI32 {
        Key = i32;
        key_field_name = n;
        insert_reducer = insert_unique_i_32;
        insert_reducer_event = InsertUniqueI32;
        delete_reducer = delete_unique_i_32;
        delete_reducer_event = DeleteUniqueI32;
    }
    UniqueI64 {
        Key = i64;
        key_field_name = n;
        insert_reducer = insert_unique_i_64;
        insert_reducer_event = InsertUniqueI64;
        delete_reducer = delete_unique_i_64;
        delete_reducer_event = DeleteUniqueI64;
    }
    UniqueI128 {
        Key = i128;
        key_field_name = n;
        insert_reducer = insert_unique_i_128;
        insert_reducer_event = InsertUniqueI128;
        delete_reducer = delete_unique_i_128;
        delete_reducer_event = DeleteUniqueI128;
    }

    UniqueBool {
        Key = bool;
        key_field_name = b;
        insert_reducer = insert_unique_bool;
        insert_reducer_event = InsertUniqueBool;
        delete_reducer = delete_unique_bool;
        delete_reducer_event = DeleteUniqueBool;
    }

    UniqueString {
        Key = String;
        key_field_name = s;
        insert_reducer = insert_unique_string;
        insert_reducer_event = InsertUniqueString;
        delete_reducer = delete_unique_string;
        delete_reducer_event = DeleteUniqueString;
    }

    UniqueIdentity {
        Key = Identity;
        key_field_name = i;
        insert_reducer = insert_unique_identity;
        insert_reducer_event = InsertUniqueIdentity;
        delete_reducer = delete_unique_identity;
        delete_reducer_event = DeleteUniqueIdentity;
    }

    UniqueAddress {
        Key = Address;
        key_field_name = a;
        insert_reducer = insert_unique_address;
        insert_reducer_event = InsertUniqueAddress;
        delete_reducer = delete_unique_address;
        delete_reducer_event = DeleteUniqueAddress;
    }
}
