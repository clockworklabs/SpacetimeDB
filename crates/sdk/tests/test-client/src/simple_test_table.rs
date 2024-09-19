use crate::module_bindings::*;
use spacetimedb_sdk::{
    sats::{i256, u256},
    Address, Event, Identity, Table,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use test_counter::TestCounter;

pub trait SimpleTestTable {
    type Contents: Clone + Send + Sync + PartialEq + std::fmt::Debug + 'static;

    fn as_contents(&self) -> &Self::Contents;

    fn is_insert_reducer_event(event: &Reducer) -> bool;

    fn insert(ctx: &impl RemoteDbContext, contents: Self::Contents);

    fn on_insert(ctx: &impl RemoteDbContext, callback: impl FnMut(&EventContext, &Self) + Send + 'static);
}

macro_rules! impl_simple_test_table {
    (__impl $table:ident {
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

            fn is_insert_reducer_event(event: &Reducer) -> bool {
                matches!(event, Reducer::$insert_reducer_event(_))
            }

            fn insert(ctx: &impl RemoteDbContext, contents: Self::Contents) {
                ctx.reducers().$insert_reducer(contents).unwrap();
            }

            fn on_insert(ctx: &impl RemoteDbContext, callback: impl FnMut(&EventContext, &Self) + Send + 'static) {
                ctx.db().$table().on_insert(callback);
            }
        }
    };
    ($($table:ident { $($stuff:tt)* })*) => {
        $(impl_simple_test_table!(__impl $table { $($stuff)* });)*
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
    OneU256 {
        Contents = u256;
        field_name = n;
        insert_reducer = insert_one_u_256;
        insert_reducer_event = InsertOneU256;
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
    OneI256 {
        Contents = i256;
        field_name = n;
        insert_reducer = insert_one_i_256;
        insert_reducer_event = InsertOneI256;
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
    VecU256 {
        Contents = Vec<u256>;
        field_name = n;
        insert_reducer = insert_vec_u_256;
        insert_reducer_event = InsertVecU256;
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
    VecI256 {
        Contents = Vec<i256>;
        field_name = n;
        insert_reducer = insert_vec_i_256;
        insert_reducer_event = InsertVecI256;
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
        field_name = v;
        insert_reducer = insert_option_vec_option_i_32;
        insert_reducer_event = InsertOptionVecOptionI32;
    }
}

pub fn on_insert_one<T: SimpleTestTable + std::fmt::Debug>(
    ctx: &impl RemoteDbContext,
    test_counter: &Arc<TestCounter>,
    value: T::Contents,
    is_expected_variant: impl Fn(&Reducer) -> bool + Send + 'static,
) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    let mut set_result = Some(test_counter.add_test(format!(
        "insert-{}-{}",
        std::any::type_name::<T>(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )));

    T::on_insert(ctx, move |ctx: &EventContext, row| {
        if let Some(set_result) = set_result.take() {
            let run_checks = || {
                anyhow::ensure!(
                    row.as_contents() == &value,
                    "Unexpected row value. Expected {value:?} but found {row:?}",
                );
                let Event::Reducer(reducer_event) = &ctx.event else {
                    anyhow::bail!("Expected a reducer event");
                };
                anyhow::ensure!(
                    is_expected_variant(&reducer_event.reducer),
                    "Unexpected Reducer variant {:?}",
                    reducer_event.reducer,
                );
                Ok(())
            };
            set_result(run_checks());
        }
    });
}

pub fn insert_one<T: SimpleTestTable + std::fmt::Debug + 'static>(
    ctx: &impl RemoteDbContext,
    test_counter: &Arc<TestCounter>,
    value: T::Contents,
) {
    on_insert_one::<T>(ctx, test_counter, value.clone(), T::is_insert_reducer_event);
    T::insert(ctx, value);
}
