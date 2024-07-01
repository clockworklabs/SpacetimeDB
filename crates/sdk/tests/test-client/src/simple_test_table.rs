use crate::module_bindings::*;
use anyhow::anyhow;
use spacetimedb_sdk::{identity::Identity, table::TableType, Address};
use std::sync::Arc;
use test_counter::TestCounter;

pub trait SimpleTestTable: TableType {
    type Contents: Clone + Send + Sync + PartialEq + std::fmt::Debug + 'static;

    fn as_contents(&self) -> &Self::Contents;

    fn is_insert_reducer_event(event: &Self::ReducerEvent) -> bool;

    fn insert(contents: Self::Contents);
}

macro_rules! impl_simple_test_table {
    ($table:ty {
        Contents = $contents:ty;
        field_name = $field_name:ident;
        insert_reducer = $insert_reducer:ident;
        insert_reducer_event = $insert_reducer_event:ident;
    }) => {
        impl SimpleTestTable for $table {
            type Contents = $contents;

            fn as_contents(&self) -> &Self::Contents {
                &self.$field_name
            }

            fn is_insert_reducer_event(event: &Self::ReducerEvent) -> bool {
                matches!(event, ReducerEvent::$insert_reducer_event(_))
            }

            fn insert(contents: Self::Contents) {
                $insert_reducer(contents);
            }
        }
    };
    ($($table:ty { $($stuff:tt)* })*) => {
        $(impl_simple_test_table!($table { $($stuff)* });)*
    };
}

impl_simple_test_table! {
    OneU8 {
        Contents = u8;
        field_name = n;
        insert_reducer = insert_one_u_8;
        insert_reducer_event = InsertOneU8;
    }
    OneU16 {
        Contents = u16;
        field_name = n;
        insert_reducer = insert_one_u_16;
        insert_reducer_event = InsertOneU16;
    }
    OneU32 {
        Contents = u32;
        field_name = n;
        insert_reducer = insert_one_u_32;
        insert_reducer_event = InsertOneU32;
    }
    OneU64 {
        Contents = u64;
        field_name = n;
        insert_reducer = insert_one_u_64;
        insert_reducer_event = InsertOneU64;
    }
    OneU128 {
        Contents = u128;
        field_name = n;
        insert_reducer = insert_one_u_128;
        insert_reducer_event = InsertOneU128;
    }

    OneI8 {
        Contents = i8;
        field_name = n;
        insert_reducer = insert_one_i_8;
        insert_reducer_event = InsertOneI8;
    }
    OneI16 {
        Contents = i16;
        field_name = n;
        insert_reducer = insert_one_i_16;
        insert_reducer_event = InsertOneI16;
    }
    OneI32 {
        Contents = i32;
        field_name = n;
        insert_reducer = insert_one_i_32;
        insert_reducer_event = InsertOneI32;
    }
    OneI64 {
        Contents = i64;
        field_name = n;
        insert_reducer = insert_one_i_64;
        insert_reducer_event = InsertOneI64;
    }
    OneI128 {
        Contents = i128;
        field_name = n;
        insert_reducer = insert_one_i_128;
        insert_reducer_event = InsertOneI128;
    }

    OneF32 {
        Contents = f32;
        field_name = f;
        insert_reducer = insert_one_f_32;
        insert_reducer_event = InsertOneF32;
    }
    OneF64 {
        Contents = f64;
        field_name = f;
        insert_reducer = insert_one_f_64;
        insert_reducer_event = InsertOneF64;
    }

    OneBool {
        Contents = bool;
        field_name = b;
        insert_reducer = insert_one_bool;
        insert_reducer_event = InsertOneBool;
    }

    OneString {
        Contents = String;
        field_name = s;
        insert_reducer = insert_one_string;
        insert_reducer_event = InsertOneString;
    }

    OneIdentity {
        Contents = Identity;
        field_name = i;
        insert_reducer = insert_one_identity;
        insert_reducer_event = InsertOneIdentity;
    }

    OneAddress {
        Contents = Address;
        field_name = a;
        insert_reducer = insert_one_address;
        insert_reducer_event = InsertOneAddress;
    }

    OneSimpleEnum {
        Contents = SimpleEnum;
        field_name = e;
        insert_reducer = insert_one_simple_enum;
        insert_reducer_event = InsertOneSimpleEnum;
    }
    OneEnumWithPayload {
        Contents = EnumWithPayload;
        field_name = e;
        insert_reducer = insert_one_enum_with_payload;
        insert_reducer_event = InsertOneEnumWithPayload;
    }

    OneUnitStruct {
        Contents = UnitStruct;
        field_name = s;
        insert_reducer = insert_one_unit_struct;
        insert_reducer_event = InsertOneUnitStruct;
    }
    OneByteStruct {
        Contents = ByteStruct;
        field_name = s;
        insert_reducer = insert_one_byte_struct;
        insert_reducer_event = InsertOneByteStruct;
    }
    OneEveryPrimitiveStruct {
        Contents = EveryPrimitiveStruct;
        field_name = s;
        insert_reducer = insert_one_every_primitive_struct;
        insert_reducer_event = InsertOneEveryPrimitiveStruct;
    }
    OneEveryVecStruct {
        Contents = EveryVecStruct;
        field_name = s;
        insert_reducer = insert_one_every_vec_struct;
        insert_reducer_event = InsertOneEveryVecStruct;
    }

    VecU8 {
        Contents = Vec<u8>;
        field_name = n;
        insert_reducer = insert_vec_u_8;
        insert_reducer_event = InsertVecU8;
    }
    VecU16 {
        Contents = Vec<u16>;
        field_name = n;
        insert_reducer = insert_vec_u_16;
        insert_reducer_event = InsertVecU16;
    }
    VecU32 {
        Contents = Vec<u32>;
        field_name = n;
        insert_reducer = insert_vec_u_32;
        insert_reducer_event = InsertVecU32;
    }
    VecU64 {
        Contents = Vec<u64>;
        field_name = n;
        insert_reducer = insert_vec_u_64;
        insert_reducer_event = InsertVecU64;
    }
    VecU128 {
        Contents = Vec<u128>;
        field_name = n;
        insert_reducer = insert_vec_u_128;
        insert_reducer_event = InsertVecU128;
    }

    VecI8 {
        Contents = Vec<i8>;
        field_name = n;
        insert_reducer = insert_vec_i_8;
        insert_reducer_event = InsertVecI8;
    }
    VecI16 {
        Contents = Vec<i16>;
        field_name = n;
        insert_reducer = insert_vec_i_16;
        insert_reducer_event = InsertVecI16;
    }
    VecI32 {
        Contents = Vec<i32>;
        field_name = n;
        insert_reducer = insert_vec_i_32;
        insert_reducer_event = InsertVecI32;
    }
    VecI64 {
        Contents = Vec<i64>;
        field_name = n;
        insert_reducer = insert_vec_i_64;
        insert_reducer_event = InsertVecI64;
    }
    VecI128 {
        Contents = Vec<i128>;
        field_name = n;
        insert_reducer = insert_vec_i_128;
        insert_reducer_event = InsertVecI128;
    }

    VecF32 {
        Contents = Vec<f32>;
        field_name = f;
        insert_reducer = insert_vec_f_32;
        insert_reducer_event = InsertVecF32;
    }
    VecF64 {
        Contents = Vec<f64>;
        field_name = f;
        insert_reducer = insert_vec_f_64;
        insert_reducer_event = InsertVecF64;
    }

    VecBool {
        Contents = Vec<bool>;
        field_name = b;
        insert_reducer = insert_vec_bool;
        insert_reducer_event = InsertVecBool;
    }

    VecString {
        Contents = Vec<String>;
        field_name = s;
        insert_reducer = insert_vec_string;
        insert_reducer_event = InsertVecString;
    }

    VecIdentity {
        Contents = Vec<Identity>;
        field_name = i;
        insert_reducer = insert_vec_identity;
        insert_reducer_event = InsertVecIdentity;
    }

    VecAddress {
        Contents = Vec<Address>;
        field_name = a;
        insert_reducer = insert_vec_address;
        insert_reducer_event = InsertVecAddress;
    }

    VecSimpleEnum {
        Contents = Vec<SimpleEnum>;
        field_name = e;
        insert_reducer = insert_vec_simple_enum;
        insert_reducer_event = InsertVecSimpleEnum;
    }
    VecEnumWithPayload {
        Contents = Vec<EnumWithPayload>;
        field_name = e;
        insert_reducer = insert_vec_enum_with_payload;
        insert_reducer_event = InsertVecEnumWithPayload;
    }

    VecUnitStruct {
        Contents = Vec<UnitStruct>;
        field_name = s;
        insert_reducer = insert_vec_unit_struct;
        insert_reducer_event = InsertVecUnitStruct;
    }
    VecByteStruct {
        Contents = Vec<ByteStruct>;
        field_name = s;
        insert_reducer = insert_vec_byte_struct;
        insert_reducer_event = InsertVecByteStruct;
    }
    VecEveryPrimitiveStruct {
        Contents = Vec<EveryPrimitiveStruct>;
        field_name = s;
        insert_reducer = insert_vec_every_primitive_struct;
        insert_reducer_event = InsertVecEveryPrimitiveStruct;
    }
    VecEveryVecStruct {
        Contents = Vec<EveryVecStruct>;
        field_name = s;
        insert_reducer = insert_vec_every_vec_struct;
        insert_reducer_event = InsertVecEveryVecStruct;
    }
    OptionI32 {
        Contents = Option<i32>;
        field_name = n;
        insert_reducer = insert_option_i_32;
        insert_reducer_event = InsertOptionI32;
    }
    OptionString {
        Contents = Option<String>;
        field_name = s;
        insert_reducer = insert_option_string;
        insert_reducer_event = InsertOptionString;
    }
    OptionIdentity {
        Contents = Option<Identity>;
        field_name = i;
        insert_reducer = insert_option_identity;
        insert_reducer_event = InsertOptionIdentity;
    }
    OptionSimpleEnum {
        Contents = Option<SimpleEnum>;
        field_name = e;
        insert_reducer = insert_option_simple_enum;
        insert_reducer_event = InsertOptionSimpleEnum;
    }
    OptionEveryPrimitiveStruct {
        Contents = Option<EveryPrimitiveStruct>;
        field_name = s;
        insert_reducer = insert_option_every_primitive_struct;
        insert_reducer_event = InsertOptionEveryPrimitiveStruct;
    }
    OptionVecOptionI32 {
        Contents = Option<Vec<Option<i32>>>;
        field_name = n;
        insert_reducer = insert_option_vec_option_i_32;
        insert_reducer_event = InsertOptionVecOptionI32;
    }
}

pub fn insert_one<T: SimpleTestTable>(test_counter: &Arc<TestCounter>, value: T::Contents) {
    let mut result = Some(test_counter.add_test(format!("insert-{}", T::TABLE_NAME)));
    let value_dup = value.clone();
    T::on_insert(move |row, reducer_event| {
        if result.is_some() {
            let run_checks = || {
                if row.as_contents() != &value_dup {
                    anyhow::bail!("Unexpected row value. Expected {:?} but found {:?}", value_dup, row);
                }
                reducer_event
                    .ok_or(anyhow!("Expected a reducer event, but found None."))
                    .map(T::is_insert_reducer_event)
                    .and_then(|is_good| is_good.then_some(()).ok_or(anyhow!("Unexpected ReducerEvent variant.")))?;

                Ok(())
            };
            (result.take().unwrap())(run_checks());
        }
    });

    T::insert(value);
}
