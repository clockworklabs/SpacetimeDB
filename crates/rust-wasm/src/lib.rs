use spacetimedb_bindgen::spacetimedb;
use spacetimedb_bindings::ColValue;

#[spacetimedb(table)]
#[spacetimedb(index(btree), name="my_index_name", my_int0, my_int1)]
pub struct MyStruct {
    #[primary_key]
    my_int0 : i32,
    my_int1 : u32,
    my_int2 : i32,
}

#[spacetimedb(reducer)]
fn my_spacetime_func(_a : i32, _b : i32) {
    println!("I am a standard function!");
}

#[spacetimedb(migrate)]
fn my_migration_fun() {

}

// __table__<name>
// __migrate__
// __reducer__<name>
// insert
// delete
// delete_eq
// delete_range - passing a range
// delete_filter - passing a function (hard)
// delete field equal
// update call should delete then insert
// indexes on single columns (do bindings as well)

// #[spacetimedb(comparer)]
// fn my_comparer() -> bool
// {
//         for entry in table {
//             if f(value) {
//                 delete();
//             }
//             // skip
//         }
//
//     let val : ColValue;
//         println!("Deleting!");
// }

// fn my_test_reducer()
// {
//     MyStruct::delete_where(|a| {
//         if a > 4 {
//             return true;
//         }
//         return false;
//     })
// }

// fn __filter__1(arg_ptr: u32, arg_size: u32) {
//
// }
//
// fn my_filter(a : MyStruct[], set : bool[]) {
//     if a.my_int2 > 4 {
//         return true;
//     }
//     return false;
// }

// impl MyStruct {
//     fn delete_filter(f : fn(MyStruct) -> bool) {
//         // for now we can iterate the rows but we have no way to specify deletion in bindings
//         let results : [bool;100];
//         while we_have_more_rows {
//             let rows = spacetimedb_bindings::read_100(MyStruct::table_id);
//             for (row, i) in rows.iter().enumerate() {
//                 let my_struct = MyStruct::parse(row);
//                 if f(my_struct) {
//                     results[i] = true;
//                 }
//             }
//
//             spacetimedb_bindings::write_100(results);
//         }
//     }
// }


#[cfg(test)]
mod tests {
    // use spacetimedb_bindings::ColValue;
    use crate::MyStruct;

    #[test]
    fn my_test() {
        // crate::__my_spacetime_func_reducer();
        // crate::my_func();
        let str = MyStruct {
            my_int0: 0,
            my_int1: 0,
            my_int2: 0
        };

        // MyStruct::insert(0, str);
        // MyStruct::delete_eq(1, 0, ColValue::I32(0));
    }
}