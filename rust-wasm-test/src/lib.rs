mod stdb;
use crate::stdb::{Column, ColValue};

#[no_mangle]
pub extern fn warmup() {}

#[no_mangle]
pub extern fn reduce(_actor: u64) {
    stdb::create_table(0, 
        Column {
            col_type: 3,
            col_id: 0,
        }, 
        Column {
            col_type: 3,
            col_id: 1,
        },
        Column {
            col_type: 3,
            col_id: 2,
        }
    );
    for i in 0..100 {
        stdb::insert(0, 
            ColValue {
                ty: 3,
                value: i,
            }, 
            ColValue {
                ty: 3,
                value: 1,
            },
            ColValue {
                ty: 3,
                value: 2,
            }
        );
    }
}