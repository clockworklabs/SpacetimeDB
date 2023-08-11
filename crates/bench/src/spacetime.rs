use crate::prelude::*;
use spacetimedb::db::datastore::traits::TableDef;
use spacetimedb::db::relational_db::{open_db, RelationalDB};
use spacetimedb_lib::auth::StTableType;
use spacetimedb_lib::sats::product;
use spacetimedb_lib::{AlgebraicType, AlgebraicValue, ProductType};
use std::env::temp_dir;
use std::path::PathBuf;

type DbResult = (RelationalDB, PathBuf, u32);

fn base_path() -> PathBuf {
    temp_dir().join("stdb_bench")
}

pub fn db_path(db_instance: usize) -> PathBuf {
    base_path().join(db_instance.to_string())
}

pub fn open_conn(path: &PathBuf) -> ResultBench<DbResult> {
    let stdb = open_db(path, false)?;

    let tx = stdb.begin_tx();

    let table_id = stdb
        .get_all_tables(&tx)?
        .iter()
        .find(|x| x.table_type == StTableType::User)
        .map(|x| x.table_id)
        .expect("Not find table Inventory");

    stdb.rollback_tx(tx);

    Ok((stdb, path.clone(), table_id))
}

pub fn create_db(db_instance: usize) -> ResultBench<PathBuf> {
    let path = db_path(db_instance);
    if path.exists() {
        std::fs::remove_dir_all(&path)?;
    }

    let stdb = open_db(&path, false)?;
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

    Ok(path)
}

pub fn create_dbs(total_dbs: usize) -> ResultBench<()> {
    let path = base_path();

    if path.exists() {
        std::fs::remove_dir_all(&path)?;
    }

    for i in 0..total_dbs {
        create_db(i)?;
    }

    set_counter(0)?;

    Ok(())
}

pub fn get_counter() -> ResultBench<usize> {
    let x = std::fs::read_to_string(base_path().join("counter"))?;
    let counter = x.parse::<usize>()?;

    Ok(counter)
}

//This is a hack to be able to select which pre-create db to use
pub fn set_counter(count: usize) -> ResultBench<()> {
    std::fs::write(base_path().join("counter"), count.to_string())?;
    Ok(())
}

pub fn insert_tx_per_row(conn: &RelationalDB, table_id: u32, run: Runs) -> ResultBench<()> {
    for row in run.data() {
        let mut tx = conn.begin_tx();

        conn.insert(
            &mut tx,
            table_id,
            product![
                AlgebraicValue::I32(row.a),
                AlgebraicValue::U64(row.b),
                AlgebraicValue::String(row.c),
            ],
        )?;

        conn.commit_tx(tx)?;
    }
    Ok(())
}

pub fn insert_tx(conn: &RelationalDB, table_id: u32, run: Runs) -> ResultBench<()> {
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

pub fn select_no_index(conn: &RelationalDB, table_id: u32, run: Runs) -> ResultBench<()> {
    let tx = conn.begin_tx();

    for i in run.range().skip(1) {
        let i = i as u64;
        let _r = conn
            .iter(&tx, table_id)?
            .map(|r| Data {
                a: *r.view().elements[0].as_i32().unwrap(),
                b: *r.view().elements[1].as_u64().unwrap(),
                c: r.view().elements[2].as_string().unwrap().clone(),
            })
            .filter(|x| x.b >= i * START_B && x.b < (START_B + (i * START_B)))
            .collect::<Vec<_>>();

        assert_eq!(_r.len() as u64, START_B);
        //dbg!(_r.len());
    }
    Ok(())
}
