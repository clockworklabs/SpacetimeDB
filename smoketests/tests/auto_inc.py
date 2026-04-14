
from .. import Smoketest
import string
import functools


ints = "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128"


def reducer_name(int_ty: str) -> str:
    # Convert "u8" -> "u_8", "i128" -> "i_128"
    return f"{int_ty[0]}_{int_ty[1:]}"


class IntTests:
    make_func = lambda int_ty: lambda self: self.do_test_autoinc(int_ty)
    for int_ty in ints:
        locals()[f"test_autoinc_{int_ty}"] = make_func(int_ty)
    del int_ty, make_func


autoinc1_template = string.Template("""
#[spacetimedb::table(accessor = person_$KEY_TY)]
pub struct Person_$KEY_TY {
    #[auto_inc]
    key_col: $KEY_TY,
    name: String,
}

#[spacetimedb::reducer]
pub fn add_$REDUCER_TY(ctx: &ReducerContext, name: String, expected_value: $KEY_TY) {
    let value = ctx.db.person_$KEY_TY().insert(Person_$KEY_TY { key_col: 0, name });
    assert_eq!(value.key_col, expected_value);
}

#[spacetimedb::reducer]
pub fn say_hello_$REDUCER_TY(ctx: &ReducerContext) {
    for person in ctx.db.person_$KEY_TY().iter() {
        log::info!("Hello, {}:{}!", person.key_col, person.name);
    }
    log::info!("Hello, World!");
}
""")


class AutoincBasic(IntTests, Smoketest):
    "This tests the auto_inc functionality"

    MODULE_CODE = f"""
#![allow(non_camel_case_types)]
use spacetimedb::{{log, ReducerContext, Table}};
{"".join(
    autoinc1_template.substitute(
        KEY_TY=int_ty,
        REDUCER_TY=reducer_name(int_ty),
    )
    for int_ty in ints
)}
"""

    def do_test_autoinc(self, int_ty):
        r = reducer_name(int_ty)
        self.call(f"add_{r}", "Robert", 1)
        self.call(f"add_{r}", "Julie", 2)
        self.call(f"add_{r}", "Samantha", 3)
        self.call(f"say_hello_{r}")
        logs = self.logs(4)
        self.assertIn("Hello, 3:Samantha!", logs)
        self.assertIn("Hello, 2:Julie!", logs)
        self.assertIn("Hello, 1:Robert!", logs)
        self.assertIn("Hello, World!", logs)


autoinc2_template = string.Template("""
#[spacetimedb::table(accessor = person_$KEY_TY)]
pub struct Person_$KEY_TY {
    #[auto_inc]
    #[unique]
    key_col: $KEY_TY,
    #[unique]
    name: String,
}

#[spacetimedb::reducer]
pub fn add_new_$REDUCER_TY(ctx: &ReducerContext, name: String) -> Result<(), Box<dyn Error>> {
    let value = ctx.db.person_$KEY_TY().try_insert(Person_$KEY_TY { key_col: 0, name })?;
    log::info!("Assigned Value: {} -> {}", value.key_col, value.name);
    Ok(())
}

#[spacetimedb::reducer]
pub fn update_$REDUCER_TY(ctx: &ReducerContext, name: String, new_id: $KEY_TY) {
    ctx.db.person_$KEY_TY().name().delete(&name);
    let _value = ctx.db.person_$KEY_TY().insert(Person_$KEY_TY { key_col: new_id, name });
}

#[spacetimedb::reducer]
pub fn say_hello_$REDUCER_TY(ctx: &ReducerContext) {
    for person in ctx.db.person_$KEY_TY().iter() {
        log::info!("Hello, {}:{}!", person.key_col, person.name);
    }
    log::info!("Hello, World!");
}
""")


class AutoincUnique(IntTests, Smoketest):
    """This tests unique constraints being violated during autoinc insertion"""

    MODULE_CODE = f"""
#![allow(non_camel_case_types)]
use std::error::Error;
use spacetimedb::{{log, ReducerContext, Table}};
{"".join(
    autoinc2_template.substitute(
        KEY_TY=int_ty,
        REDUCER_TY=reducer_name(int_ty),
    )
    for int_ty in ints
)}
"""

    def do_test_autoinc(self, int_ty):
        r = reducer_name(int_ty)
        self.call(f"update_{r}", "Robert", 2)
        self.call(f"add_new_{r}", "Success")
        with self.assertRaises(Exception):
            self.call(f"add_new_{r}", "Failure")

        self.call(f"say_hello_{r}")
        logs = self.logs(4)
        self.assertIn("Hello, 2:Robert!", logs)
        self.assertIn("Hello, 1:Success!", logs)
        self.assertIn("Hello, World!", logs)
