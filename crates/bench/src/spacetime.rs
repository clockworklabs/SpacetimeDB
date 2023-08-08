use crate::prelude::*;
use spacetimedb::db::datastore::locking_tx_datastore::MutTxId;
use spacetimedb::db::datastore::traits::TableDef;
use spacetimedb::db::relational_db::{open_db, RelationalDB};
use spacetimedb_lib::sats::product;
use spacetimedb_lib::{AlgebraicType, AlgebraicValue, ProductType};

type DbResult = (RelationalDB, TempDir, u32);

fn init_db(in_memory: bool, fsync: bool) -> ResultBench<(TempDir, u32)> {
    let tmp_dir = TempDir::new("stdb_test")?;
    let stdb = open_db(tmp_dir.path(), in_memory, fsync)?;
    let mut tx = stdb.begin_tx();
    let table_id = stdb.create_table(
        &mut tx,
        TableDef::from(ProductType::from_iter([
            ("a", AlgebraicType::I32),
            ("b", AlgebraicType::U64),
            ("c", AlgebraicType::String),
        ])),
    )?;
    stdb.commit_tx(tx)?;
    Ok((tmp_dir, table_id))
}

fn build_db(in_memory: bool, fsync: bool) -> ResultBench<DbResult> {
    let (tmp_dir, table_id) = init_db(in_memory, fsync)?;
    let stdb = open_db(&tmp_dir, in_memory, fsync)?;
    Ok((stdb, tmp_dir, table_id))
}

fn insert_row(db: &RelationalDB, tx: &mut MutTxId, table_id: u32, run: Runs) -> ResultBench<()> {
    for row in run.data() {
        db.insert(
            tx,
            table_id,
            product![
                AlgebraicValue::I32(row.a),
                AlgebraicValue::U64(row.b),
                AlgebraicValue::String(row.c),
            ],
        )?;
    }

    Ok(())
}

impl BuildDb for DbResult {
    fn build(prefill: bool, fsync: bool) -> ResultBench<Self>
    where
        Self: Sized,
    {
        // For benchmarking, we are concerned with the persistent version of the database.
        let in_memory = false;
        let db = build_db(in_memory, fsync)?;

        if prefill {
            prefill_data(&db, Runs::Small)?;
        }
        Ok(db)
    }
}

pub fn prefill_data(db: &DbResult, run: Runs) -> ResultBench<()> {
    let (conn, _tmp_dir, table_id) = db;

    let mut tx = conn.begin_tx();
    insert_row(conn, &mut tx, *table_id, run)?;

    conn.commit_tx(tx)?;
    Ok(())
}

pub fn insert_tx_per_row(pool: &mut Pool<DbResult>, run: Runs) -> ResultBench<()> {
    let (conn, _tmp_dir, table_id) = pool.next()?;

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

pub fn insert_tx(pool: &mut Pool<DbResult>, _run: Runs) -> ResultBench<()> {
    pool.prefill = true;
    pool.next()?;
    Ok(())
}

pub fn select_no_index(pool: &mut Pool<DbResult>, run: Runs) -> ResultBench<()> {
    let (conn, _tmp_dir, table_id) = pool.next()?;

    let tx = conn.begin_tx();

    for i in run.range().skip(1) {
        let i = i as u64;
        let _r = conn
            .iter(&tx, table_id)?
            .map(|r| Data {
                a: *r.view().elements[0].as_i32().unwrap(),
                b: *r.view().elements[1].as_u64().unwrap(),
                c: r.view().elements[2].as_string().unwrap().into(),
            })
            .filter(|x| x.b >= i * START_B && x.b < (START_B + (i * START_B)))
            .collect::<Vec<_>>();

        assert_eq!(_r.len() as u64, START_B);
        //dbg!(_r.len());
    }
    Ok(())
}
