use crate::prelude::*;
use rusqlite::Connection;
use std::env::temp_dir;
use std::path::PathBuf;

pub fn db_path() -> PathBuf {
    let mut tmp_dir = temp_dir();
    tmp_dir.push("sqlite_bench");
    tmp_dir
}

pub fn db_path_instance(db_instance: usize) -> PathBuf {
    db_path().join(format!("{db_instance}.db"))
}

pub fn open_conn(path: &PathBuf) -> ResultBench<Connection> {
    let db = Connection::open(path)?;
    db.execute_batch(
        "PRAGMA journal_mode = WAL;
            PRAGMA synchronous = normal;",
    )?;

    Ok(db)
}

pub fn create_db(db_instance: usize) -> ResultBench<PathBuf> {
    let path = db_path();
    if !path.exists() {
        std::fs::create_dir_all(&path)?;
    }
    let path = db_path_instance(db_instance);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    let db = open_conn(&path)?;

    db.execute_batch(
        "CREATE TABLE data (
            a INTEGER PRIMARY KEY,
            b BIGINT NOT NULL,
            c TEXT);",
    )?;

    Ok(path)
}

pub fn create_dbs(total_dbs: usize) -> ResultBench<()> {
    let path = db_path();

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
    let x = std::fs::read_to_string(db_path().join("counter"))?;
    let counter = x.parse::<usize>()?;

    Ok(counter)
}

//This is a hack to be able to select which pre-create db to use
pub fn set_counter(count: usize) -> ResultBench<()> {
    std::fs::write(db_path().join("counter"), count.to_string())?;
    Ok(())
}

pub fn insert_tx_per_row(conn: &mut Connection, run: Runs) -> ResultBench<()> {
    for row in run.data() {
        conn.execute(
            &format!("INSERT INTO data VALUES({} ,{}, '{}');", row.a, row.b, row.c),
            (),
        )?;
    }
    Ok(())
}

pub fn insert_tx(conn: &mut Connection, run: Runs) -> ResultBench<()> {
    let tx = conn.transaction()?;

    for row in run.data() {
        tx.execute(
            &format!("INSERT INTO data VALUES({} ,{}, '{}');", row.a, row.b, row.c),
            (),
        )?;
    }

    tx.commit()?;

    Ok(())
}

pub fn select_no_index(conn: &mut Connection, run: Runs) -> ResultBench<()> {
    for i in run.range().skip(1) {
        let i = i as u64;
        let sql = &format!(
            "SELECT * FROM data WHERE b >= {} AND b < {}",
            i * START_B,
            START_B + (i * START_B)
        );

        //dbg!(sql);
        let mut stmt = conn.prepare(sql)?;
        let _r = stmt
            .query_map([], |row| {
                Ok(Data {
                    a: row.get(0)?,
                    b: row.get(1)?,
                    c: row.get(2)?,
                })
            })?
            .collect::<Vec<_>>();
        assert_eq!(_r.len() as u64, START_B);
        //dbg!(_r.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() -> ResultBench<()> {
        let run = Runs::Tiny;
        let path = create_db(0).unwrap();
        let mut conn = sqlite::open_conn(&path).unwrap();
        insert_tx_per_row(&mut conn, run).unwrap();
        Ok(())
    }
}
