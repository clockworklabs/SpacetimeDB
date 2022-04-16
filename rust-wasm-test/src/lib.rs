use spacetimedb_bindings::*;

/*
TODO:
Handle strings
Handle structs
Handle contract parameters supplied from host
Impl reading from the db
Impl schema code-gen
Impl stdb as a server
Impl uploading new contract
*/

#[no_mangle]
pub extern fn reduce(_actor: u64) {
    create_table(0, vec![
        Column { col_id: 0, col_type: ColType::U32 },
        Column { col_id: 1, col_type: ColType::U32 },
        Column { col_id: 2, col_type: ColType::U32 },
    ]);
    for i in 0..100 {
        insert(0, vec![
            ColValue::U32(i),
            ColValue::U32(1),
            ColValue::U32(2),
        ]);
    }
}