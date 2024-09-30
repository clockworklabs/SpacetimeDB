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
        accessor_method = $accessor_method:ident;
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
                ctx.db().$accessor_method().on_insert(callback);
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
        accessor_method = one_u_8;
    }
    OneU16 {
        Contents = u16;
        field_name = n;
        insert_reducer = insert_one_u_16;
        insert_reducer_event = InsertOneU16;
        accessor_method = one_u_16;
    }
    OneU32 {
        Contents = u32;
        field_name = n;
        insert_reducer = insert_one_u_32;
        insert_reducer_event = InsertOneU32;
        accessor_method = one_u_32;
    }
    OneU64 {
        Contents = u64;
        field_name = n;
        insert_reducer = insert_one_u_64;
        insert_reducer_event = InsertOneU64;
        accessor_method = one_u_64;
    }
    OneU128 {
        Contents = u128;
        field_name = n;
        insert_reducer = insert_one_u_128;
        insert_reducer_event = InsertOneU128;
        accessor_method = one_u_128;
    }
    OneU256 {
        Contents = u256;
        field_name = n;
        insert_reducer = insert_one_u_256;
        insert_reducer_event = InsertOneU256;
        accessor_method = one_u_256;
    }

    OneI8 {
        Contents = i8;
        field_name = n;
        insert_reducer = insert_one_i_8;
        insert_reducer_event = InsertOneI8;
        accessor_method = one_i_8;
    }
    OneI16 {
        Contents = i16;
        field_name = n;
        insert_reducer = insert_one_i_16;
        insert_reducer_event = InsertOneI16;
        accessor_method = one_i_16;
    }
    OneI32 {
        Contents = i32;
        field_name = n;
        insert_reducer = insert_one_i_32;
        insert_reducer_event = InsertOneI32;
        accessor_method = one_i_32;
    }
    OneI64 {
        Contents = i64;
        field_name = n;
        insert_reducer = insert_one_i_64;
        insert_reducer_event = InsertOneI64;
        accessor_method = one_i_64;
    }
    OneI128 {
        Contents = i128;
        field_name = n;
        insert_reducer = insert_one_i_128;
        insert_reducer_event = InsertOneI128;
        accessor_method = one_i_128;
    }
    OneI256 {
        Contents = i256;
        field_name = n;
        insert_reducer = insert_one_i_256;
        insert_reducer_event = InsertOneI256;
        accessor_method = one_i_256;
    }

    OneF32 {
        Contents = f32;
        field_name = f;
        insert_reducer = insert_one_f_32;
        insert_reducer_event = InsertOneF32;
        accessor_method = one_f_32;
    }
    OneF64 {
        Contents = f64;
        field_name = f;
        insert_reducer = insert_one_f_64;
        insert_reducer_event = InsertOneF64;
        accessor_method = one_f_64;
    }

    OneBool {
        Contents = bool;
        field_name = b;
        insert_reducer = insert_one_bool;
        insert_reducer_event = InsertOneBool;
        accessor_method = one_bool;
    }

    OneString {
        Contents = String;
        field_name = s;
        insert_reducer = insert_one_string;
        insert_reducer_event = InsertOneString;
        accessor_method = one_string;
    }

    OneIdentity {
        Contents = Identity;
        field_name = i;
        insert_reducer = insert_one_identity;
        insert_reducer_event = InsertOneIdentity;
        accessor_method = one_identity;
    }

    OneAddress {
        Contents = Address;
        field_name = a;
        insert_reducer = insert_one_address;
        insert_reducer_event = InsertOneAddress;
        accessor_method = one_address;
    }

    OneSimpleEnum {
        Contents = SimpleEnum;
        field_name = e;
        insert_reducer = insert_one_simple_enum;
        insert_reducer_event = InsertOneSimpleEnum;
        accessor_method = one_simple_enum;
    }
    OneEnumWithPayload {
        Contents = EnumWithPayload;
        field_name = e;
        insert_reducer = insert_one_enum_with_payload;
        insert_reducer_event = InsertOneEnumWithPayload;
        accessor_method = one_enum_with_payload;
    }

    OneUnitStruct {
        Contents = UnitStruct;
        field_name = s;
        insert_reducer = insert_one_unit_struct;
        insert_reducer_event = InsertOneUnitStruct;
        accessor_method = one_unit_struct;
    }
    OneByteStruct {
        Contents = ByteStruct;
        field_name = s;
        insert_reducer = insert_one_byte_struct;
        insert_reducer_event = InsertOneByteStruct;
        accessor_method = one_byte_struct;
    }
    OneEveryPrimitiveStruct {
        Contents = EveryPrimitiveStruct;
        field_name = s;
        insert_reducer = insert_one_every_primitive_struct;
        insert_reducer_event = InsertOneEveryPrimitiveStruct;
        accessor_method = one_every_primitive_struct;
    }
    OneEveryVecStruct {
        Contents = EveryVecStruct;
        field_name = s;
        insert_reducer = insert_one_every_vec_struct;
        insert_reducer_event = InsertOneEveryVecStruct;
        accessor_method = one_every_vec_struct;
    }

    VecU8 {
        Contents = Vec<u8>;
        field_name = n;
        insert_reducer = insert_vec_u_8;
        insert_reducer_event = InsertVecU8;
        accessor_method = vec_u_8;
    }
    VecU16 {
        Contents = Vec<u16>;
        field_name = n;
        insert_reducer = insert_vec_u_16;
        insert_reducer_event = InsertVecU16;
        accessor_method = vec_u_16;
    }
    VecU32 {
        Contents = Vec<u32>;
        field_name = n;
        insert_reducer = insert_vec_u_32;
        insert_reducer_event = InsertVecU32;
        accessor_method = vec_u_32;
    }
    VecU64 {
        Contents = Vec<u64>;
        field_name = n;
        insert_reducer = insert_vec_u_64;
        insert_reducer_event = InsertVecU64;
        accessor_method = vec_u_64;
    }
    VecU128 {
        Contents = Vec<u128>;
        field_name = n;
        insert_reducer = insert_vec_u_128;
        insert_reducer_event = InsertVecU128;
        accessor_method = vec_u_128;
    }
    VecU256 {
        Contents = Vec<u256>;
        field_name = n;
        insert_reducer = insert_vec_u_256;
        insert_reducer_event = InsertVecU256;
        accessor_method = vec_u_256;
    }

    VecI8 {
        Contents = Vec<i8>;
        field_name = n;
        insert_reducer = insert_vec_i_8;
        insert_reducer_event = InsertVecI8;
        accessor_method = vec_i_8;
    }
    VecI16 {
        Contents = Vec<i16>;
        field_name = n;
        insert_reducer = insert_vec_i_16;
        insert_reducer_event = InsertVecI16;
        accessor_method = vec_i_16;
    }
    VecI32 {
        Contents = Vec<i32>;
        field_name = n;
        insert_reducer = insert_vec_i_32;
        insert_reducer_event = InsertVecI32;
        accessor_method = vec_i_32;
    }
    VecI64 {
        Contents = Vec<i64>;
        field_name = n;
        insert_reducer = insert_vec_i_64;
        insert_reducer_event = InsertVecI64;
        accessor_method = vec_i_64;
    }
    VecI128 {
        Contents = Vec<i128>;
        field_name = n;
        insert_reducer = insert_vec_i_128;
        insert_reducer_event = InsertVecI128;
        accessor_method = vec_i_128;
    }
    VecI256 {
        Contents = Vec<i256>;
        field_name = n;
        insert_reducer = insert_vec_i_256;
        insert_reducer_event = InsertVecI256;
        accessor_method = vec_i_256;
    }

    VecF32 {
        Contents = Vec<f32>;
        field_name = f;
        insert_reducer = insert_vec_f_32;
        insert_reducer_event = InsertVecF32;
        accessor_method = vec_f_32;
    }
    VecF64 {
        Contents = Vec<f64>;
        field_name = f;
        insert_reducer = insert_vec_f_64;
        insert_reducer_event = InsertVecF64;
        accessor_method = vec_f_64;
    }

    VecBool {
        Contents = Vec<bool>;
        field_name = b;
        insert_reducer = insert_vec_bool;
        insert_reducer_event = InsertVecBool;
        accessor_method = vec_bool;
    }

    VecString {
        Contents = Vec<String>;
        field_name = s;
        insert_reducer = insert_vec_string;
        insert_reducer_event = InsertVecString;
        accessor_method = vec_string;
    }

    VecIdentity {
        Contents = Vec<Identity>;
        field_name = i;
        insert_reducer = insert_vec_identity;
        insert_reducer_event = InsertVecIdentity;
        accessor_method = vec_identity;
    }

    VecAddress {
        Contents = Vec<Address>;
        field_name = a;
        insert_reducer = insert_vec_address;
        insert_reducer_event = InsertVecAddress;
        accessor_method = vec_address;
    }

    VecSimpleEnum {
        Contents = Vec<SimpleEnum>;
        field_name = e;
        insert_reducer = insert_vec_simple_enum;
        insert_reducer_event = InsertVecSimpleEnum;
        accessor_method = vec_simple_enum;
    }
    VecEnumWithPayload {
        Contents = Vec<EnumWithPayload>;
        field_name = e;
        insert_reducer = insert_vec_enum_with_payload;
        insert_reducer_event = InsertVecEnumWithPayload;
        accessor_method = vec_enum_with_payload;
    }

    VecUnitStruct {
        Contents = Vec<UnitStruct>;
        field_name = s;
        insert_reducer = insert_vec_unit_struct;
        insert_reducer_event = InsertVecUnitStruct;
        accessor_method = vec_unit_struct;
    }
    VecByteStruct {
        Contents = Vec<ByteStruct>;
        field_name = s;
        insert_reducer = insert_vec_byte_struct;
        insert_reducer_event = InsertVecByteStruct;
        accessor_method = vec_byte_struct;
    }
    VecEveryPrimitiveStruct {
        Contents = Vec<EveryPrimitiveStruct>;
        field_name = s;
        insert_reducer = insert_vec_every_primitive_struct;
        insert_reducer_event = InsertVecEveryPrimitiveStruct;
        accessor_method = vec_every_primitive_struct;
    }
    VecEveryVecStruct {
        Contents = Vec<EveryVecStruct>;
        field_name = s;
        insert_reducer = insert_vec_every_vec_struct;
        insert_reducer_event = InsertVecEveryVecStruct;
        accessor_method = vec_every_vec_struct;
    }
    OptionI32 {
        Contents = Option<i32>;
        field_name = n;
        insert_reducer = insert_option_i_32;
        insert_reducer_event = InsertOptionI32;
        accessor_method = option_i_32;
    }
    OptionString {
        Contents = Option<String>;
        field_name = s;
        insert_reducer = insert_option_string;
        insert_reducer_event = InsertOptionString;
        accessor_method = option_string;
    }
    OptionIdentity {
        Contents = Option<Identity>;
        field_name = i;
        insert_reducer = insert_option_identity;
        insert_reducer_event = InsertOptionIdentity;
        accessor_method = option_identity;
    }
    OptionSimpleEnum {
        Contents = Option<SimpleEnum>;
        field_name = e;
        insert_reducer = insert_option_simple_enum;
        insert_reducer_event = InsertOptionSimpleEnum;
        accessor_method = option_simple_enum;
    }
    OptionEveryPrimitiveStruct {
        Contents = Option<EveryPrimitiveStruct>;
        field_name = s;
        insert_reducer = insert_option_every_primitive_struct;
        insert_reducer_event = InsertOptionEveryPrimitiveStruct;
        accessor_method = option_every_primitive_struct;
    }
    OptionVecOptionI32 {
        Contents = Option<Vec<Option<i32>>>;
        field_name = v;
        insert_reducer = insert_option_vec_option_i_32;
        insert_reducer_event = InsertOptionVecOptionI32;
        accessor_method = option_vec_option_i_32;
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
