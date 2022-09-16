#![feature(prelude_import)]
#[prelude_import]
use std::prelude::rust_2021::*;
#[macro_use]
extern crate std;
use spacetimedb_bindgen::spacetimedb;
use spacetimedb_bindings::println;
use spacetimedb_bindings::{delete_range, Hash, RangeTypeValue};
use std::time::Duration;
#[allow(non_upper_case_globals)]
static mut __table_id__TestA: Option<u32> = None;
#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn __create_table__TestA(arg_ptr: usize, arg_size: usize) {
    let def = TestA::get_struct_schema();
    if let spacetimedb_bindings::TypeDef::Tuple(tuple_def) = def {
        let table_id = spacetimedb_bindings::create_table("TestA", tuple_def);
        unsafe { __table_id__TestA = Some(table_id) }
    } else {
        ::core::panicking::panic_fmt(::core::fmt::Arguments::new_v1(
            &["This type is not a tuple: {#original_struct_ident}"],
            &[],
        ));
    }
}
#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn __describe_table__TestA() -> u64 {
    let def = TestA::get_struct_schema();
    if let spacetimedb_bindings::TypeDef::Tuple(tuple_def) = def {
        let mut bytes = ::alloc::vec::Vec::new();
        tuple_def.encode(&mut bytes);
        let offset = bytes.as_ptr() as u64;
        let length = bytes.len() as u64;
        std::mem::forget(bytes);
        return offset << 32 | length;
    } else {
        ::core::panicking::panic_fmt(::core::fmt::Arguments::new_v1(
            &["This type is not a tuple: {#original_struct_ident}"],
            &[],
        ));
    }
}
pub struct TestA {
    pub x: u32,
    pub y: u32,
    pub z: String,
}
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl _serde::Serialize for TestA {
        fn serialize<__S>(&self, __serializer: __S) -> _serde::__private::Result<__S::Ok, __S::Error>
        where
            __S: _serde::Serializer,
        {
            let mut __serde_state =
                match _serde::Serializer::serialize_struct(__serializer, "TestA", false as usize + 1 + 1 + 1) {
                    _serde::__private::Ok(__val) => __val,
                    _serde::__private::Err(__err) => {
                        return _serde::__private::Err(__err);
                    }
                };
            match _serde::ser::SerializeStruct::serialize_field(&mut __serde_state, "x", &self.x) {
                _serde::__private::Ok(__val) => __val,
                _serde::__private::Err(__err) => {
                    return _serde::__private::Err(__err);
                }
            };
            match _serde::ser::SerializeStruct::serialize_field(&mut __serde_state, "y", &self.y) {
                _serde::__private::Ok(__val) => __val,
                _serde::__private::Err(__err) => {
                    return _serde::__private::Err(__err);
                }
            };
            match _serde::ser::SerializeStruct::serialize_field(&mut __serde_state, "z", &self.z) {
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
    impl<'de> _serde::Deserialize<'de> for TestA {
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
                fn expecting(&self, __formatter: &mut _serde::__private::Formatter) -> _serde::__private::fmt::Result {
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
                fn visit_str<__E>(self, __value: &str) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        "x" => _serde::__private::Ok(__Field::__field0),
                        "y" => _serde::__private::Ok(__Field::__field1),
                        "z" => _serde::__private::Ok(__Field::__field2),
                        _ => _serde::__private::Ok(__Field::__ignore),
                    }
                }
                fn visit_bytes<__E>(self, __value: &[u8]) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        b"x" => _serde::__private::Ok(__Field::__field0),
                        b"y" => _serde::__private::Ok(__Field::__field1),
                        b"z" => _serde::__private::Ok(__Field::__field2),
                        _ => _serde::__private::Ok(__Field::__ignore),
                    }
                }
            }
            impl<'de> _serde::Deserialize<'de> for __Field {
                #[inline]
                fn deserialize<__D>(__deserializer: __D) -> _serde::__private::Result<Self, __D::Error>
                where
                    __D: _serde::Deserializer<'de>,
                {
                    _serde::Deserializer::deserialize_identifier(__deserializer, __FieldVisitor)
                }
            }
            struct __Visitor<'de> {
                marker: _serde::__private::PhantomData<TestA>,
                lifetime: _serde::__private::PhantomData<&'de ()>,
            }
            impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                type Value = TestA;
                fn expecting(&self, __formatter: &mut _serde::__private::Formatter) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "struct TestA")
                }
                #[inline]
                fn visit_seq<__A>(self, mut __seq: __A) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::SeqAccess<'de>,
                {
                    let __field0 = match match _serde::de::SeqAccess::next_element::<u32>(&mut __seq) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    } {
                        _serde::__private::Some(__value) => __value,
                        _serde::__private::None => {
                            return _serde::__private::Err(_serde::de::Error::invalid_length(
                                0usize,
                                &"struct TestA with 3 elements",
                            ));
                        }
                    };
                    let __field1 = match match _serde::de::SeqAccess::next_element::<u32>(&mut __seq) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    } {
                        _serde::__private::Some(__value) => __value,
                        _serde::__private::None => {
                            return _serde::__private::Err(_serde::de::Error::invalid_length(
                                1usize,
                                &"struct TestA with 3 elements",
                            ));
                        }
                    };
                    let __field2 = match match _serde::de::SeqAccess::next_element::<String>(&mut __seq) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    } {
                        _serde::__private::Some(__value) => __value,
                        _serde::__private::None => {
                            return _serde::__private::Err(_serde::de::Error::invalid_length(
                                2usize,
                                &"struct TestA with 3 elements",
                            ));
                        }
                    };
                    _serde::__private::Ok(TestA {
                        x: __field0,
                        y: __field1,
                        z: __field2,
                    })
                }
                #[inline]
                fn visit_map<__A>(self, mut __map: __A) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::MapAccess<'de>,
                {
                    let mut __field0: _serde::__private::Option<u32> = _serde::__private::None;
                    let mut __field1: _serde::__private::Option<u32> = _serde::__private::None;
                    let mut __field2: _serde::__private::Option<String> = _serde::__private::None;
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
                                    return _serde::__private::Err(<__A::Error as _serde::de::Error>::duplicate_field(
                                        "x",
                                    ));
                                }
                                __field0 = _serde::__private::Some(
                                    match _serde::de::MapAccess::next_value::<u32>(&mut __map) {
                                        _serde::__private::Ok(__val) => __val,
                                        _serde::__private::Err(__err) => {
                                            return _serde::__private::Err(__err);
                                        }
                                    },
                                );
                            }
                            __Field::__field1 => {
                                if _serde::__private::Option::is_some(&__field1) {
                                    return _serde::__private::Err(<__A::Error as _serde::de::Error>::duplicate_field(
                                        "y",
                                    ));
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
                                    return _serde::__private::Err(<__A::Error as _serde::de::Error>::duplicate_field(
                                        "z",
                                    ));
                                }
                                __field2 = _serde::__private::Some(
                                    match _serde::de::MapAccess::next_value::<String>(&mut __map) {
                                        _serde::__private::Ok(__val) => __val,
                                        _serde::__private::Err(__err) => {
                                            return _serde::__private::Err(__err);
                                        }
                                    },
                                );
                            }
                            _ => {
                                let _ = match _serde::de::MapAccess::next_value::<_serde::de::IgnoredAny>(&mut __map) {
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
                        _serde::__private::None => match _serde::__private::de::missing_field("x") {
                            _serde::__private::Ok(__val) => __val,
                            _serde::__private::Err(__err) => {
                                return _serde::__private::Err(__err);
                            }
                        },
                    };
                    let __field1 = match __field1 {
                        _serde::__private::Some(__field1) => __field1,
                        _serde::__private::None => match _serde::__private::de::missing_field("y") {
                            _serde::__private::Ok(__val) => __val,
                            _serde::__private::Err(__err) => {
                                return _serde::__private::Err(__err);
                            }
                        },
                    };
                    let __field2 = match __field2 {
                        _serde::__private::Some(__field2) => __field2,
                        _serde::__private::None => match _serde::__private::de::missing_field("z") {
                            _serde::__private::Ok(__val) => __val,
                            _serde::__private::Err(__err) => {
                                return _serde::__private::Err(__err);
                            }
                        },
                    };
                    _serde::__private::Ok(TestA {
                        x: __field0,
                        y: __field1,
                        z: __field2,
                    })
                }
            }
            const FIELDS: &'static [&'static str] = &["x", "y", "z"];
            _serde::Deserializer::deserialize_struct(
                __deserializer,
                "TestA",
                FIELDS,
                __Visitor {
                    marker: _serde::__private::PhantomData::<TestA>,
                    lifetime: _serde::__private::PhantomData,
                },
            )
        }
    }
};
pub struct TestAIter {
    iter: spacetimedb_bindings::TableIter,
}
impl Iterator for TestAIter {
    type Item = TestA;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(tuple) = self.iter.next() {
            Some(TestA::tuple_to_struct(tuple).expect("Failed to convert tuple to struct."))
        } else {
            None
        }
    }
}
impl TestA {
    #[allow(unused_variables)]
    pub fn insert(ins: TestA) {
        spacetimedb_bindings::insert(
            Self::table_id(),
            spacetimedb_bindings::TupleValue {
                elements: <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        spacetimedb_bindings::TypeValue::U32(ins.x),
                        spacetimedb_bindings::TypeValue::U32(ins.y),
                        spacetimedb_bindings::TypeValue::String(ins.z),
                    ]),
                ),
            },
        );
    }
    #[allow(unused_variables)]
    pub fn delete(f: fn(TestA) -> bool) -> usize {
        ::core::panicking::panic_fmt(::core::fmt::Arguments::new_v1(
            &["Delete using a function is not supported yet!"],
            &[],
        ));
    }
    #[allow(unused_variables)]
    pub fn update(value: TestA) -> bool {
        ::core::panicking::panic_fmt(::core::fmt::Arguments::new_v1(
            &["Update using a value is not supported yet!"],
            &[],
        ));
    }
    #[allow(unused_variables)]
    pub fn iter() -> TestAIter {
        TestAIter {
            iter: Self::iter_tuples(),
        }
    }
    #[allow(unused_variables)]
    pub fn iter_tuples() -> spacetimedb_bindings::TableIter {
        spacetimedb_bindings::__iter__(Self::table_id()).expect("Failed to get iterator from table.")
    }
    #[allow(non_snake_case)]
    #[allow(unused_variables)]
    pub fn filter_x_eq(x: u32) -> Vec<TestA> {
        let mut result = Vec::<TestA>::new();
        let table_iter = TestA::iter_tuples();
        for row in table_iter {
            let column_data = row.elements[0usize].clone();
            if let spacetimedb_bindings::TypeValue::U32(entry_data) = column_data.clone() {
                if entry_data == x {
                    let tuple = Self::tuple_to_struct(row);
                    if let None = tuple {
                        ::spacetimedb_bindings::io::_console_log_info(&{
                            let res = ::alloc::fmt::format(::core::fmt::Arguments::new_v1(
                                &["Internal stdb error: Can\'t convert from tuple to struct (wrong version?) TestA"],
                                &[],
                            ));
                            res
                        });
                        continue;
                    }
                    result.push(tuple.unwrap());
                }
            }
        }
        return result;
    }
    #[allow(non_snake_case)]
    #[allow(unused_variables)]
    pub fn filter_y_eq(y: u32) -> Vec<TestA> {
        let mut result = Vec::<TestA>::new();
        let table_iter = TestA::iter_tuples();
        for row in table_iter {
            let column_data = row.elements[1usize].clone();
            if let spacetimedb_bindings::TypeValue::U32(entry_data) = column_data.clone() {
                if entry_data == y {
                    let tuple = Self::tuple_to_struct(row);
                    if let None = tuple {
                        ::spacetimedb_bindings::io::_console_log_info(&{
                            let res = ::alloc::fmt::format(::core::fmt::Arguments::new_v1(
                                &["Internal stdb error: Can\'t convert from tuple to struct (wrong version?) TestA"],
                                &[],
                            ));
                            res
                        });
                        continue;
                    }
                    result.push(tuple.unwrap());
                }
            }
        }
        return result;
    }
    #[allow(non_snake_case)]
    #[allow(unused_variables)]
    pub fn filter_z_eq(z: String) -> Vec<TestA> {
        let mut result = Vec::<TestA>::new();
        let table_iter = TestA::iter_tuples();
        for row in table_iter {
            let column_data = row.elements[2usize].clone();
            if let spacetimedb_bindings::TypeValue::String(entry_data) = column_data.clone() {
                if entry_data == z {
                    let tuple = Self::tuple_to_struct(row);
                    if let None = tuple {
                        ::spacetimedb_bindings::io::_console_log_info(&{
                            let res = ::alloc::fmt::format(::core::fmt::Arguments::new_v1(
                                &["Internal stdb error: Can\'t convert from tuple to struct (wrong version?) TestA"],
                                &[],
                            ));
                            res
                        });
                        continue;
                    }
                    result.push(tuple.unwrap());
                }
            }
        }
        return result;
    }
    #[allow(non_snake_case)]
    pub fn tuple_to_struct(value: spacetimedb_bindings::TupleValue) -> Option<TestA> {
        let elements_arr = value.elements;
        return match (
            elements_arr[0usize].clone(),
            elements_arr[1usize].clone(),
            elements_arr[2usize].clone(),
        ) {
            (
                spacetimedb_bindings::TypeValue::U32(field_0),
                spacetimedb_bindings::TypeValue::U32(field_1),
                spacetimedb_bindings::TypeValue::String(field_2),
            ) => Some(TestA {
                x: field_0,
                y: field_1,
                z: field_2,
            }),
            _ => None,
        };
    }
    #[allow(non_snake_case)]
    pub fn struct_to_tuple(value: TestA) -> spacetimedb_bindings::TypeValue {
        return spacetimedb_bindings::TypeValue::Tuple(spacetimedb_bindings::TupleValue {
            elements: <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    spacetimedb_bindings::TypeValue::U32(value.x),
                    spacetimedb_bindings::TypeValue::U32(value.y),
                    spacetimedb_bindings::TypeValue::String(value.z),
                ]),
            ),
        });
    }
    #[allow(non_snake_case)]
    pub fn get_struct_schema() -> spacetimedb_bindings::TypeDef {
        return spacetimedb_bindings::TypeDef::Tuple {
            0: spacetimedb_bindings::TupleDef {
                elements: <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        spacetimedb_bindings::ElementDef {
                            tag: 0u8,
                            name: Some("x".to_string()),
                            element_type: spacetimedb_bindings::TypeDef::U32,
                        },
                        spacetimedb_bindings::ElementDef {
                            tag: 1u8,
                            name: Some("y".to_string()),
                            element_type: spacetimedb_bindings::TypeDef::U32,
                        },
                        spacetimedb_bindings::ElementDef {
                            tag: 2u8,
                            name: Some("z".to_string()),
                            element_type: spacetimedb_bindings::TypeDef::String,
                        },
                    ]),
                ),
            },
        };
    }
    pub fn table_id() -> u32 {
        if let Some(t_id) = unsafe { __table_id__TestA } {
            return t_id;
        }
        let t_id = spacetimedb_bindings::get_table_id("TestA");
        unsafe { __table_id__TestA = Some(t_id) };
        return t_id;
    }
}
pub struct TestB {
    foo: String,
}
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl _serde::Serialize for TestB {
        fn serialize<__S>(&self, __serializer: __S) -> _serde::__private::Result<__S::Ok, __S::Error>
        where
            __S: _serde::Serializer,
        {
            let mut __serde_state =
                match _serde::Serializer::serialize_struct(__serializer, "TestB", false as usize + 1) {
                    _serde::__private::Ok(__val) => __val,
                    _serde::__private::Err(__err) => {
                        return _serde::__private::Err(__err);
                    }
                };
            match _serde::ser::SerializeStruct::serialize_field(&mut __serde_state, "foo", &self.foo) {
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
    impl<'de> _serde::Deserialize<'de> for TestB {
        fn deserialize<__D>(__deserializer: __D) -> _serde::__private::Result<Self, __D::Error>
        where
            __D: _serde::Deserializer<'de>,
        {
            #[allow(non_camel_case_types)]
            enum __Field {
                __field0,
                __ignore,
            }
            struct __FieldVisitor;
            impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                type Value = __Field;
                fn expecting(&self, __formatter: &mut _serde::__private::Formatter) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "field identifier")
                }
                fn visit_u64<__E>(self, __value: u64) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        0u64 => _serde::__private::Ok(__Field::__field0),
                        _ => _serde::__private::Ok(__Field::__ignore),
                    }
                }
                fn visit_str<__E>(self, __value: &str) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        "foo" => _serde::__private::Ok(__Field::__field0),
                        _ => _serde::__private::Ok(__Field::__ignore),
                    }
                }
                fn visit_bytes<__E>(self, __value: &[u8]) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        b"foo" => _serde::__private::Ok(__Field::__field0),
                        _ => _serde::__private::Ok(__Field::__ignore),
                    }
                }
            }
            impl<'de> _serde::Deserialize<'de> for __Field {
                #[inline]
                fn deserialize<__D>(__deserializer: __D) -> _serde::__private::Result<Self, __D::Error>
                where
                    __D: _serde::Deserializer<'de>,
                {
                    _serde::Deserializer::deserialize_identifier(__deserializer, __FieldVisitor)
                }
            }
            struct __Visitor<'de> {
                marker: _serde::__private::PhantomData<TestB>,
                lifetime: _serde::__private::PhantomData<&'de ()>,
            }
            impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                type Value = TestB;
                fn expecting(&self, __formatter: &mut _serde::__private::Formatter) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "struct TestB")
                }
                #[inline]
                fn visit_seq<__A>(self, mut __seq: __A) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::SeqAccess<'de>,
                {
                    let __field0 = match match _serde::de::SeqAccess::next_element::<String>(&mut __seq) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    } {
                        _serde::__private::Some(__value) => __value,
                        _serde::__private::None => {
                            return _serde::__private::Err(_serde::de::Error::invalid_length(
                                0usize,
                                &"struct TestB with 1 element",
                            ));
                        }
                    };
                    _serde::__private::Ok(TestB { foo: __field0 })
                }
                #[inline]
                fn visit_map<__A>(self, mut __map: __A) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::MapAccess<'de>,
                {
                    let mut __field0: _serde::__private::Option<String> = _serde::__private::None;
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
                                    return _serde::__private::Err(<__A::Error as _serde::de::Error>::duplicate_field(
                                        "foo",
                                    ));
                                }
                                __field0 = _serde::__private::Some(
                                    match _serde::de::MapAccess::next_value::<String>(&mut __map) {
                                        _serde::__private::Ok(__val) => __val,
                                        _serde::__private::Err(__err) => {
                                            return _serde::__private::Err(__err);
                                        }
                                    },
                                );
                            }
                            _ => {
                                let _ = match _serde::de::MapAccess::next_value::<_serde::de::IgnoredAny>(&mut __map) {
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
                        _serde::__private::None => match _serde::__private::de::missing_field("foo") {
                            _serde::__private::Ok(__val) => __val,
                            _serde::__private::Err(__err) => {
                                return _serde::__private::Err(__err);
                            }
                        },
                    };
                    _serde::__private::Ok(TestB { foo: __field0 })
                }
            }
            const FIELDS: &'static [&'static str] = &["foo"];
            _serde::Deserializer::deserialize_struct(
                __deserializer,
                "TestB",
                FIELDS,
                __Visitor {
                    marker: _serde::__private::PhantomData::<TestB>,
                    lifetime: _serde::__private::PhantomData,
                },
            )
        }
    }
};
impl TestB {
    #[allow(non_snake_case)]
    pub fn tuple_to_struct(value: spacetimedb_bindings::TupleValue) -> Option<TestB> {
        let elements_arr = value.elements;
        return match (elements_arr[0usize].clone()) {
            (spacetimedb_bindings::TypeValue::String(field_0)) => Some(TestB { foo: field_0 }),
            _ => None,
        };
    }
    #[allow(non_snake_case)]
    pub fn struct_to_tuple(value: TestB) -> spacetimedb_bindings::TypeValue {
        return spacetimedb_bindings::TypeValue::Tuple(spacetimedb_bindings::TupleValue {
            elements: <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([spacetimedb_bindings::TypeValue::String(value.foo)]),
            ),
        });
    }
    #[allow(non_snake_case)]
    pub fn get_struct_schema() -> spacetimedb_bindings::TypeDef {
        return spacetimedb_bindings::TypeDef::Tuple {
            0: spacetimedb_bindings::TupleDef {
                elements: <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([spacetimedb_bindings::ElementDef {
                        tag: 0u8,
                        name: Some("foo".to_string()),
                        element_type: spacetimedb_bindings::TypeDef::String,
                    }]),
                ),
            },
        };
    }
}
#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn __create_type__TestB(arg_ptr: usize, arg_size: usize) {
    let ptr = unsafe { arg_ptr as *mut u8 };
    let def = TestB::get_struct_schema();
    let mut bytes = unsafe { Vec::from_raw_parts(ptr, 0, arg_size) };
    def.encode(&mut bytes);
}
#[allow(non_snake_case)]
pub extern "C" fn __migrate__(arg_ptr: u32, arg_size: u32) {
    migrate();
}
pub fn migrate() {}
#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn __repeating_reducer__repeating_test(arg_ptr: usize, arg_size: usize) -> u64 {
    let arguments =
        spacetimedb_bindings::args::RepeatingReducerArguments::decode_mem(unsafe { arg_ptr as *mut u8 }, arg_size)
            .expect("Unable to decode module arguments");
    repeating_test(arguments.timestamp, arguments.delta_time);
    return 1000u64;
}
pub fn repeating_test(timestamp: u64, delta_time: u64) {
    let delta_time = Duration::from_millis(delta_time);
    let timestamp = Duration::from_millis(timestamp);
    ::spacetimedb_bindings::io::_console_log_info(&{
        let res = ::alloc::fmt::format(::core::fmt::Arguments::new_v1(
            &["Timestamp: ", ", Delta time: "],
            &[
                ::core::fmt::ArgumentV1::new_debug(&timestamp),
                ::core::fmt::ArgumentV1::new_debug(&delta_time),
            ],
        ));
        res
    });
}
#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn __reducer__test(arg_ptr: usize, arg_size: usize) {
    let arguments = spacetimedb_bindings::args::ReducerArguments::decode_mem(unsafe { arg_ptr as *mut u8 }, arg_size)
        .expect("Unable to decode module arguments");
    let arg_json: serde_json::Value = serde_json::from_slice(arguments.argument_bytes.as_slice()).expect(
        {
            let res = ::alloc::fmt::format(::core::fmt::Arguments::new_v1(
                &["Unable to parse arguments as JSON: ", " bytes/arg_size: ", ": "],
                &[
                    ::core::fmt::ArgumentV1::new_display(&arguments.argument_bytes.len()),
                    ::core::fmt::ArgumentV1::new_display(&arg_size),
                    ::core::fmt::ArgumentV1::new_debug(&arguments.argument_bytes),
                ],
            ));
            res
        }
        .as_str(),
    );
    let args = arg_json.as_array().expect("Unable to extract reducer arguments list");
    let arg_2: TestA = serde_json::from_value(args[0usize].clone()).unwrap();
    let arg_3: TestB = serde_json::from_value(args[1usize].clone()).unwrap();
    test(arguments.identity, arguments.timestamp, arg_2, arg_3);
}
#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn __describe_reducer__test() -> u64 {
    let tupledef = spacetimedb_bindings::TupleDef {
        elements: <[_]>::into_vec(
            #[rustc_box]
            ::alloc::boxed::Box::new([
                spacetimedb_bindings::ElementDef {
                    tag: 0u8,
                    name: Some("sender".to_string()),
                    element_type: spacetimedb_bindings::TypeDef::Bytes,
                },
                spacetimedb_bindings::ElementDef {
                    tag: 1u8,
                    name: Some("timestamp".to_string()),
                    element_type: spacetimedb_bindings::TypeDef::U64,
                },
                spacetimedb_bindings::ElementDef {
                    tag: 2u8,
                    name: Some("arg".to_string()),
                    element_type: TestA::get_struct_schema(),
                },
                spacetimedb_bindings::ElementDef {
                    tag: 3u8,
                    name: Some("arg2".to_string()),
                    element_type: TestB::get_struct_schema(),
                },
            ]),
        ),
    };
    let mut bytes = ::alloc::vec::Vec::new();
    tupledef.encode(&mut bytes);
    let offset = bytes.as_ptr() as u64;
    let length = bytes.len() as u64;
    std::mem::forget(bytes);
    return offset << 32 | length;
}
pub fn test(sender: Hash, timestamp: u64, arg: TestA, arg2: TestB) {
    ::spacetimedb_bindings::io::_console_log_info(&{
        let res = ::alloc::fmt::format(::core::fmt::Arguments::new_v1(&["BEGIN"], &[]));
        res
    });
    ::spacetimedb_bindings::io::_console_log_info(&{
        let res = ::alloc::fmt::format(::core::fmt::Arguments::new_v1(
            &["sender: "],
            &[::core::fmt::ArgumentV1::new_debug(&sender)],
        ));
        res
    });
    ::spacetimedb_bindings::io::_console_log_info(&{
        let res = ::alloc::fmt::format(::core::fmt::Arguments::new_v1(
            &["timestamp: "],
            &[::core::fmt::ArgumentV1::new_display(&timestamp)],
        ));
        res
    });
    ::spacetimedb_bindings::io::_console_log_info(&{
        let res = ::alloc::fmt::format(::core::fmt::Arguments::new_v1(
            &["bar: "],
            &[::core::fmt::ArgumentV1::new_debug(&arg2.foo)],
        ));
        res
    });
    for i in 0..10 {
        TestA::insert(TestA {
            x: i + arg.x,
            y: i + arg.y,
            z: "Yo".to_owned(),
        });
    }
    let mut row_count = 0;
    for _row in TestA::iter() {
        row_count += 1;
    }
    ::spacetimedb_bindings::io::_console_log_info(&{
        let res = ::alloc::fmt::format(::core::fmt::Arguments::new_v1(
            &["Row count before delete: "],
            &[::core::fmt::ArgumentV1::new_debug(&row_count)],
        ));
        res
    });
    delete_range(1, 0, RangeTypeValue::U32(5)..RangeTypeValue::U32(10));
    let mut row_count = 0;
    for _row in TestA::iter() {
        row_count += 1;
    }
    ::spacetimedb_bindings::io::_console_log_info(&{
        let res = ::alloc::fmt::format(::core::fmt::Arguments::new_v1(
            &["Row count after delete: "],
            &[::core::fmt::ArgumentV1::new_debug(&row_count)],
        ));
        res
    });
    ::spacetimedb_bindings::io::_console_log_info(&{
        let res = ::alloc::fmt::format(::core::fmt::Arguments::new_v1(&["END"], &[]));
        res
    });
}
