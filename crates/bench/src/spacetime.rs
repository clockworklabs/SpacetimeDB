use crate::prelude::*;
use spacetimedb::db::message_log::MessageLog;
use spacetimedb::db::relational_db::{open_db, open_log, RelationalDB};
use spacetimedb::db::transactional_db::Tx;
use spacetimedb_lib::{TupleDef, TupleValue, TypeDef, TypeValue};
use std::sync::{Arc, Mutex};

type DbResult = (RelationalDB, Arc<Mutex<MessageLog>>, u32);

fn init_db() -> ResultBench<(TempDir, u32)> {
    let tmp_dir = TempDir::new(&format!("stdb_test"))?;
    let mut stdb = open_db(tmp_dir.path())?;
    let mut tx_ = stdb.begin_tx();
    let (tx, stdb) = tx_.get();
    let table_id = stdb.create_table(
        tx,
        "data",
        TupleDef::from_iter([("a", TypeDef::I32), ("b", TypeDef::U64), ("c", TypeDef::String)]),
    )?;
    let log = open_log(&tmp_dir)?;

    let commit_result = tx_.commit()?;
    RelationalDB::persist_tx(&log, commit_result)?;
    Ok((tmp_dir, table_id))
}

fn build_db() -> ResultBench<DbResult> {
    let (tmp_dir, table_id) = init_db()?;
    let stdb = open_db(&tmp_dir)?;
    let log = open_log(&tmp_dir)?;

    Ok((stdb, log, table_id))
}

fn insert(db: &mut RelationalDB, tx: &mut Tx, table_id: u32, run: Runs) -> ResultBench<()> {
    for row in run.data() {
        db.insert(
            tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(row.a), TypeValue::U64(row.b), TypeValue::String(row.c)].into(),
            },
        )?;
    }

    Ok(())
}

impl BuildDb for DbResult {
    fn build(prefill: bool) -> ResultBench<Self>
    where
        Self: Sized,
    {
        let mut db = build_db()?;

        if prefill {
            prefill_data(&mut db, Runs::Small)?;
        }
        Ok(db)
    }
}

pub fn prefill_data(db: &mut DbResult, run: Runs) -> ResultBench<()> {
    let (conn, log, table_id) = db;

    let mut tx_ = conn.begin_tx();
    let (tx, db) = tx_.get();
    insert(db, tx, *table_id, run)?;

    let commit_result = tx_.commit()?;
    RelationalDB::persist_tx(&log, commit_result)?;

    Ok(())
}

pub fn insert_tx_per_row(pool: &mut Pool<DbResult>, run: Runs) -> ResultBench<()> {
    let (mut conn, log, table_id) = pool.next()?;
    //let mut log = log.lock().unwrap();
    for row in run.data() {
        let mut tx_ = conn.begin_tx();
        let (tx, db) = tx_.get();

        db.insert(
            tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(row.a), TypeValue::U64(row.b), TypeValue::String(row.c)].into(),
            },
        )?;
        let commit_result = tx_.commit()?;
        RelationalDB::persist_tx(&log, commit_result)?;
    }
    Ok(())
}

pub fn insert_tx(pool: &mut Pool<DbResult>, _run: Runs) -> ResultBench<()> {
    pool.prefill = true;
    pool.next()?;
    Ok(())
}

pub fn select_no_index(pool: &mut Pool<DbResult>, run: Runs) -> ResultBench<()> {
    let (mut conn, _, table_id) = pool.next()?;

    let mut tx_ = conn.begin_tx();
    let (tx, db) = tx_.get();

    for i in run.range().skip(1) {
        let i = i as u64;
        let _r = db
            .scan(tx, table_id)?
            .map(|r| Data {
                a: *r.elements[0].as_i32().unwrap(),
                b: *r.elements[1].as_u64().unwrap(),
                c: r.elements[2].as_string().unwrap().clone(),
            })
            .filter(|x| x.b >= i * START_B && x.b < (START_B + (i * START_B)))
            .collect::<Vec<_>>();

        assert_eq!(_r.len() as u64, START_B);
        //dbg!(_r.len());
    }
    Ok(())
}
