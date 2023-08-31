use crate::prelude::*;
use rusqlite::{Connection, Transaction};
use spacetimedb_lib::sats::SatsString;

impl BuildDb for Connection {
    fn build(prefill: bool, fsync: bool) -> ResultBench<Self>
    where
        Self: Sized,
    {
        let tmp_dir = TempDir::new("sqlite_test")?;
        let mut db = Connection::open(tmp_dir.path().join("test.db"))?;
        // For sqlite benchmarks we should set synchronous to either full or off which more
        // closely aligns with wal_fsync=true and wal_fsync=false respectively in stdb.
        db.execute_batch(if fsync {
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = full;"
        } else {
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = off;"
        })?;

        db.execute_batch(
            "CREATE TABLE data (
            a INTEGER PRIMARY KEY,
            b BIGINT NOT NULL,
            c TEXT);",
        )?;

        if prefill {
            prefill_data(&mut db, Runs::Small)?;
        }
        Ok(db)
    }
}

fn insert(db: &Transaction, run: Runs) -> ResultBench<()> {
    for row in run.data() {
        db.execute(
            &format!("INSERT INTO data (a, b, c) VALUES({} ,{}, '{}');", row.a, row.b, row.c),
            (),
        )?;
    }

    Ok(())
}

pub fn prefill_data(db: &mut Connection, run: Runs) -> ResultBench<()> {
    let tx = db.transaction()?;

    insert(&tx, run)?;

    tx.commit()?;

    Ok(())
}

pub fn insert_tx_per_row(pool: &mut Pool<Connection>, run: Runs) -> ResultBench<()> {
    let db = pool.next()?;
    for row in run.data() {
        db.execute(
            &format!("INSERT INTO data VALUES({} ,{}, '{}');", row.a, row.b, row.c),
            (),
        )?;
    }
    Ok(())
}

pub fn insert_tx(pool: &mut Pool<Connection>, _run: Runs) -> ResultBench<()> {
    pool.next()?;

    Ok(())
}

pub fn select_no_index(pool: &mut Pool<Connection>, run: Runs) -> ResultBench<()> {
    let db = pool.next()?;
    for i in run.range().skip(1) {
        let i = i as u64;
        let sql = &format!(
            "SELECT * FROM data WHERE b >= {} AND b < {}",
            i * START_B,
            START_B + (i * START_B)
        );

        //dbg!(sql);
        let mut stmt = db.prepare(sql)?;
        let _r = stmt
            .query_map([], |row| {
                Ok(Data {
                    a: row.get(0)?,
                    b: row.get(1)?,
                    c: SatsString::from_string(row.get(2)?),
                })
            })?
            .collect::<Vec<_>>();
        assert_eq!(_r.len() as u64, START_B);
        //dbg!(_r.len());
    }
    Ok(())
}
