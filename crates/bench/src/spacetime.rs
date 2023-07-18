use crate::prelude::*;
use spacetimedb::db::datastore::traits::TableDef;
use spacetimedb::db::relational_db::{open_db, RelationalDB};
use spacetimedb_lib::auth::StTableType;
use spacetimedb_lib::sats::product;
use spacetimedb_lib::{AlgebraicType, AlgebraicValue, ProductType};
use std::env::temp_dir;
use std::path::PathBuf;

type DbResult = (RelationalDB, PathBuf, u32);

pub fn db_path(db_instance: usize) -> PathBuf {
    let mut tmp_dir = temp_dir();
    tmp_dir.push("stdb_bench");
    tmp_dir.push(db_instance.to_string());
    tmp_dir
}

pub fn open_conn(path: &PathBuf) -> ResultBench<DbResult> {
    let stdb = open_db(path)?;

    let tx = stdb.begin_tx();

    let table_id = stdb
        .get_all_tables(&tx)?
        .iter()
        .find(|x| x.table_type == StTableType::User)
        .map(|x| x.table_id)
        .unwrap();

    stdb.rollback_tx(tx);

    Ok((stdb, path.clone(), table_id))
}

pub fn create_db(db_instance: usize) -> ResultBench<()> {
    let path = db_path(db_instance);
    if path.exists() {
        std::fs::remove_dir_all(&path)?;
    }

    let stdb = open_db(&path)?;
    let mut tx = stdb.begin_tx();

    stdb.create_table(
        &mut tx,
        TableDef::from(ProductType::from_iter([
            ("a", AlgebraicType::I32),
            ("b", AlgebraicType::U64),
            ("c", AlgebraicType::String),
        ])),
    )?;

    stdb.commit_tx(tx)?;

    Ok(())
}
//
// fn insert_row(db: &RelationalDB, tx: &mut MutTxId, table_id: u32, run: Runs) -> ResultBench<()> {
//     for row in run.data() {
//         db.insert_raw(
//             tx,
//             table_id,
//             product![
//                 AlgebraicValue::I32(row.a),
//                 AlgebraicValue::U64(row.b),
//                 AlgebraicValue::String(row.c),
//             ],
//         )?;
//     }
//
//     Ok(())
// }
//
// impl BuildDb for DbResult {
//     fn build(prefill: bool) -> ResultBench<Self>
//     where
//         Self: Sized,
//     {
//         let db = init_db("test", prefill)?;
//
//         Ok(db)
//     }
// }

pub fn insert_tx_per_row(conn: &RelationalDB, table_id: u32, run: Runs) -> ResultBench<()> {
    let mut tx = conn.begin_tx();

    for row in run.data() {
        conn.insert(
            &mut tx,
            table_id,
            product![
                AlgebraicValue::I32(row.a),
                AlgebraicValue::U64(row.b),
                AlgebraicValue::String(row.c),
            ],
        )?;
    }
    conn.commit_tx(tx)?;
    Ok(())
}
//
// pub fn insert_tx(pool: &mut Pool<DbResult>, _run: Runs) -> ResultBench<()> {
//     pool.prefill = true;
//     pool.next()?;
//     Ok(())
// }
//
// pub fn select_no_index(pool: &mut Pool<DbResult>, run: Runs) -> ResultBench<()> {
//     let (conn, _tmp_dir, table_id) = pool.next()?;
//
//     let tx = conn.begin_tx();
//
//     for i in run.range().skip(1) {
//         let i = i as u64;
//         let _r = conn
//             .iter(&tx, table_id)?
//             .map(|r| Data {
//                 a: *r.view().elements[0].as_i32().unwrap(),
//                 b: *r.view().elements[1].as_u64().unwrap(),
//                 c: r.view().elements[2].as_string().unwrap().clone(),
//             })
//             .filter(|x| x.b >= i * START_B && x.b < (START_B + (i * START_B)))
//             .collect::<Vec<_>>();
//
//         assert_eq!(_r.len() as u64, START_B);
//         //dbg!(_r.len());
//     }
//     Ok(())
// }
