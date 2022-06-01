#![feature(prelude_import)]
#[prelude_import]
use std::prelude::rust_2021::*;
#[macro_use]
extern crate std;
use spacetimedb_bindgen::spacetimedb;
use spacetimedb_bindings::ColValue;
pub struct MyStruct {
    #[primary_key]
    my_int0: i32,
    my_int1: u32,
    my_int2: i32,
}
impl MyStruct {
    fn __create_index__my_index_name(arg_ptr: u32, arg_size: u32) {
        unsafe {
            spacetimedb_bindings::create_index(
                __table_id__MyStruct,
                0u8,
                <[_]>::into_vec(box [0u32, 1u32]),
            );
        }
    }
}
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl _serde::Serialize for MyStruct {
        fn serialize<__S>(
            &self,
            __serializer: __S,
        ) -> _serde::__private::Result<__S::Ok, __S::Error>
        where
            __S: _serde::Serializer,
        {
            let mut __serde_state = match _serde::Serializer::serialize_struct(
                __serializer,
                "MyStruct",
                false as usize + 1 + 1 + 1,
            ) {
                _serde::__private::Ok(__val) => __val,
                _serde::__private::Err(__err) => {
                    return _serde::__private::Err(__err);
                }
            };
            match _serde::ser::SerializeStruct::serialize_field(
                &mut __serde_state,
                "my_int0",
                &self.my_int0,
            ) {
                _serde::__private::Ok(__val) => __val,
                _serde::__private::Err(__err) => {
                    return _serde::__private::Err(__err);
                }
            };
            match _serde::ser::SerializeStruct::serialize_field(
                &mut __serde_state,
                "my_int1",
                &self.my_int1,
            ) {
                _serde::__private::Ok(__val) => __val,
                _serde::__private::Err(__err) => {
                    return _serde::__private::Err(__err);
                }
            };
            match _serde::ser::SerializeStruct::serialize_field(
                &mut __serde_state,
                "my_int2",
                &self.my_int2,
            ) {
                _serde::__private::Ok(__val) => __val,
                _serde::__private::Err(__err) => {
                    return _serde::__private::Err(__err);
                }
            };
            _serde::ser::SerializeStruct::end(__serde_state)
        }
    }
};
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<'de> _serde::Deserialize<'de> for MyStruct {
        fn deserialize<__D>(__deserializer: __D) -> _serde::__private::Result<Self, __D::Error>
        where
            __D: _serde::Deserializer<'de>,
        {
            #[allow(non_camel_case_types)]
            enum __Field {
                __field0,
                __field1,
                __field2,
                __ignore,
            }
            struct __FieldVisitor;
            impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                type Value = __Field;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "field identifier")
                }
                fn visit_u64<__E>(self, __value: u64) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        0u64 => _serde::__private::Ok(__Field::__field0),
                        1u64 => _serde::__private::Ok(__Field::__field1),
                        2u64 => _serde::__private::Ok(__Field::__field2),
                        _ => _serde::__private::Ok(__Field::__ignore),
                    }
                }
                fn visit_str<__E>(
                    self,
                    __value: &str,
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        "my_int0" => _serde::__private::Ok(__Field::__field0),
                        "my_int1" => _serde::__private::Ok(__Field::__field1),
                        "my_int2" => _serde::__private::Ok(__Field::__field2),
                        _ => _serde::__private::Ok(__Field::__ignore),
                    }
                }
                fn visit_bytes<__E>(
                    self,
                    __value: &[u8],
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        b"my_int0" => _serde::__private::Ok(__Field::__field0),
                        b"my_int1" => _serde::__private::Ok(__Field::__field1),
                        b"my_int2" => _serde::__private::Ok(__Field::__field2),
                        _ => _serde::__private::Ok(__Field::__ignore),
                    }
                }
            }
            impl<'de> _serde::Deserialize<'de> for __Field {
                #[inline]
                fn deserialize<__D>(
                    __deserializer: __D,
                ) -> _serde::__private::Result<Self, __D::Error>
                where
                    __D: _serde::Deserializer<'de>,
                {
                    _serde::Deserializer::deserialize_identifier(__deserializer, __FieldVisitor)
                }
            }
            struct __Visitor<'de> {
                marker: _serde::__private::PhantomData<MyStruct>,
                lifetime: _serde::__private::PhantomData<&'de ()>,
            }
            impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                type Value = MyStruct;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "struct MyStruct")
                }
                #[inline]
                fn visit_seq<__A>(
                    self,
                    mut __seq: __A,
                ) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::SeqAccess<'de>,
                {
                    let __field0 =
                        match match _serde::de::SeqAccess::next_element::<i32>(&mut __seq) {
                            _serde::__private::Ok(__val) => __val,
                            _serde::__private::Err(__err) => {
                                return _serde::__private::Err(__err);
                            }
                        } {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(_serde::de::Error::invalid_length(
                                    0usize,
                                    &"struct MyStruct with 3 elements",
                                ));
                            }
                        };
                    let __field1 =
                        match match _serde::de::SeqAccess::next_element::<u32>(&mut __seq) {
                            _serde::__private::Ok(__val) => __val,
                            _serde::__private::Err(__err) => {
                                return _serde::__private::Err(__err);
                            }
                        } {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(_serde::de::Error::invalid_length(
                                    1usize,
                                    &"struct MyStruct with 3 elements",
                                ));
                            }
                        };
                    let __field2 =
                        match match _serde::de::SeqAccess::next_element::<i32>(&mut __seq) {
                            _serde::__private::Ok(__val) => __val,
                            _serde::__private::Err(__err) => {
                                return _serde::__private::Err(__err);
                            }
                        } {
                            _serde::__private::Some(__value) => __value,
                            _serde::__private::None => {
                                return _serde::__private::Err(_serde::de::Error::invalid_length(
                                    2usize,
                                    &"struct MyStruct with 3 elements",
                                ));
                            }
                        };
                    _serde::__private::Ok(MyStruct {
                        my_int0: __field0,
                        my_int1: __field1,
                        my_int2: __field2,
                    })
                }
                #[inline]
                fn visit_map<__A>(
                    self,
                    mut __map: __A,
                ) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::MapAccess<'de>,
                {
                    let mut __field0: _serde::__private::Option<i32> = _serde::__private::None;
                    let mut __field1: _serde::__private::Option<u32> = _serde::__private::None;
                    let mut __field2: _serde::__private::Option<i32> = _serde::__private::None;
                    while let _serde::__private::Some(__key) =
                        match _serde::de::MapAccess::next_key::<__Field>(&mut __map) {
                            _serde::__private::Ok(__val) => __val,
                            _serde::__private::Err(__err) => {
                                return _serde::__private::Err(__err);
                            }
                        }
                    {
                        match __key {
                            __Field::__field0 => {
                                if _serde::__private::Option::is_some(&__field0) {
                                    return _serde::__private::Err(
                                        <__A::Error as _serde::de::Error>::duplicate_field(
                                            "my_int0",
                                        ),
                                    );
                                }
                                __field0 = _serde::__private::Some(
                                    match _serde::de::MapAccess::next_value::<i32>(&mut __map) {
                                        _serde::__private::Ok(__val) => __val,
                                        _serde::__private::Err(__err) => {
                                            return _serde::__private::Err(__err);
                                        }
                                    },
                                );
                            }
                            __Field::__field1 => {
                                if _serde::__private::Option::is_some(&__field1) {
                                    return _serde::__private::Err(
                                        <__A::Error as _serde::de::Error>::duplicate_field(
                                            "my_int1",
                                        ),
                                    );
                                }
                                __field1 = _serde::__private::Some(
                                    match _serde::de::MapAccess::next_value::<u32>(&mut __map) {
                                        _serde::__private::Ok(__val) => __val,
                                        _serde::__private::Err(__err) => {
                                            return _serde::__private::Err(__err);
                                        }
                                    },
                                );
                            }
                            __Field::__field2 => {
                                if _serde::__private::Option::is_some(&__field2) {
                                    return _serde::__private::Err(
                                        <__A::Error as _serde::de::Error>::duplicate_field(
                                            "my_int2",
                                        ),
                                    );
                                }
                                __field2 = _serde::__private::Some(
                                    match _serde::de::MapAccess::next_value::<i32>(&mut __map) {
                                        _serde::__private::Ok(__val) => __val,
                                        _serde::__private::Err(__err) => {
                                            return _serde::__private::Err(__err);
                                        }
                                    },
                                );
                            }
                            _ => {
                                let _ = match _serde::de::MapAccess::next_value::<
                                    _serde::de::IgnoredAny,
                                >(&mut __map)
                                {
                                    _serde::__private::Ok(__val) => __val,
                                    _serde::__private::Err(__err) => {
                                        return _serde::__private::Err(__err);
                                    }
                                };
                            }
                        }
                    }
                    let __field0 = match __field0 {
                        _serde::__private::Some(__field0) => __field0,
                        _serde::__private::None => {
                            match _serde::__private::de::missing_field("my_int0") {
                                _serde::__private::Ok(__val) => __val,
                                _serde::__private::Err(__err) => {
                                    return _serde::__private::Err(__err);
                                }
                            }
                        }
                    };
                    let __field1 = match __field1 {
                        _serde::__private::Some(__field1) => __field1,
                        _serde::__private::None => {
                            match _serde::__private::de::missing_field("my_int1") {
                                _serde::__private::Ok(__val) => __val,
                                _serde::__private::Err(__err) => {
                                    return _serde::__private::Err(__err);
                                }
                            }
                        }
                    };
                    let __field2 = match __field2 {
                        _serde::__private::Some(__field2) => __field2,
                        _serde::__private::None => {
                            match _serde::__private::de::missing_field("my_int2") {
                                _serde::__private::Ok(__val) => __val,
                                _serde::__private::Err(__err) => {
                                    return _serde::__private::Err(__err);
                                }
                            }
                        }
                    };
                    _serde::__private::Ok(MyStruct {
                        my_int0: __field0,
                        my_int1: __field1,
                        my_int2: __field2,
                    })
                }
            }
            const FIELDS: &'static [&'static str] = &["my_int0", "my_int1", "my_int2"];
            _serde::Deserializer::deserialize_struct(
                __deserializer,
                "MyStruct",
                FIELDS,
                __Visitor {
                    marker: _serde::__private::PhantomData::<MyStruct>,
                    lifetime: _serde::__private::PhantomData,
                },
            )
        }
    }
};
#[no_mangle]
pub extern "C" fn __create_table__MyStruct(table_id: u32) {
    unsafe {
        __table_id__MyStruct = table_id;
    }
    spacetimedb_bindings::create_table(
        table_id,
        <[_]>::into_vec(box [
            spacetimedb_bindings::Column {
                col_id: 0u32,
                col_type: spacetimedb_bindings::ColType::I32,
            },
            spacetimedb_bindings::Column {
                col_id: 1u32,
                col_type: spacetimedb_bindings::ColType::U32,
            },
            spacetimedb_bindings::Column {
                col_id: 2u32,
                col_type: spacetimedb_bindings::ColType::I32,
            },
        ]),
    );
}
static mut __table_id__MyStruct: u32 = 0;
impl MyStruct {
    pub fn delete(f: fn(MyStruct) -> bool) -> usize {
        0
    }
    pub fn update(value: MyStruct) -> bool {
        false
    }
    fn table_row_to_struct(entry: Vec<ColValue>) -> Option<MyStruct> {
        return match (entry[0usize], entry[1usize], entry[2usize]) {
            (
                spacetimedb_bindings::ColValue::I32(my_int0),
                spacetimedb_bindings::ColValue::U32(my_int1),
                spacetimedb_bindings::ColValue::I32(my_int2),
            ) => Some(MyStruct {
                my_int0,
                my_int1,
                my_int2,
            }),
            _ => None,
        };
    }
    pub fn filter_my_int0_eq(my_int0: i32) -> Option<MyStruct> {
        unsafe {
            let table_iter = spacetimedb_bindings::iter(__table_id__MyStruct);
            if let Some(table_iter) = table_iter {
                for entry in table_iter {
                    let data = entry[0usize];
                    if let spacetimedb_bindings::ColValue::I32(data) = data {
                        if my_int0 == data {
                            let value = MyStruct::table_row_to_struct(entry);
                            if let Some(value) = value {
                                return Some(value);
                            }
                        }
                    }
                }
            }
        }
        return None;
    }
    pub fn delete_my_int0_eq(my_int0: i32) -> bool {
        false
    }
    pub fn filter_my_int1_eq(my_int1: u32) -> Vec<MyStruct> {
        unsafe {
            let mut result = Vec::<MyStruct>::new();
            let table_iter = spacetimedb_bindings::iter(__table_id__MyStruct);
            if let Some(table_iter) = table_iter {
                for entry in table_iter {
                    let data = entry[0usize];
                    if let spacetimedb_bindings::ColValue::U32(data) = data {
                        if my_int1 == data {
                            let value = MyStruct::table_row_to_struct(entry);
                            if let Some(value) = value {
                                result.push(value);
                            }
                        }
                    }
                }
            }
            return result;
        }
    }
    pub fn delete_my_int1_eq(my_int1: u32) -> usize {
        0
    }
    pub fn filter_my_int2_eq(my_int2: i32) -> Vec<MyStruct> {
        unsafe {
            let mut result = Vec::<MyStruct>::new();
            let table_iter = spacetimedb_bindings::iter(__table_id__MyStruct);
            if let Some(table_iter) = table_iter {
                for entry in table_iter {
                    let data = entry[1usize];
                    if let spacetimedb_bindings::ColValue::I32(data) = data {
                        if my_int2 == data {
                            let value = MyStruct::table_row_to_struct(entry);
                            if let Some(value) = value {
                                result.push(value);
                            }
                        }
                    }
                }
            }
            return result;
        }
    }
    pub fn delete_my_int2_eq(my_int2: i32) -> usize {
        0
    }
}
#[no_mangle]
pub extern "C" fn __reducer__my_spacetime_func(arg_ptr: u32, arg_size: u32) {
    let arg_ptr = arg_ptr as *mut u8;
    let bytes: Vec<u8> =
        unsafe { Vec::from_raw_parts(arg_ptr, arg_size as usize, arg_size as usize) };
    let arg_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let args = arg_json.as_array().unwrap();
    let arg_0: i32 = serde_json::from_value(args[0usize].clone()).unwrap();
    let arg_1: i32 = serde_json::from_value(args[1usize].clone()).unwrap();
    my_spacetime_func(arg_0, arg_1);
}
fn my_spacetime_func(_a: i32, _b: i32) {
    {
        ::std::io::_print(::core::fmt::Arguments::new_v1(
            &["I am a standard function!\n"],
            &[],
        ));
    };
}
pub extern "C" fn __migrate__(arg_ptr: u32, arg_size: u32) {
    my_migration_fun();
}
fn my_migration_fun() {}
