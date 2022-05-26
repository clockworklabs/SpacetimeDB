use criterion::{criterion_group, criterion_main, Criterion};
use spacetimedb::db::transactional_db::TransactionalDB;
use tempdir::TempDir;

fn transactional_db(c: &mut Criterion) {
    c.bench_function("tx commit", |bench| {
        let tmp_dir = TempDir::new("txdb_bench").unwrap();
        let mut db = TransactionalDB::open(tmp_dir.path()).unwrap();
        bench.iter(move || {
            let mut tx = db.begin_tx();
            let bytes = b"test".to_vec();
            db.insert(&mut tx, 0, bytes);
            assert!(db.commit_tx(tx));
        });
    });
    c.bench_function("seek", |bench| {
        let tmp_dir = TempDir::new("txdb_bench").unwrap();
        let mut db = TransactionalDB::open(tmp_dir.path()).unwrap();
        let mut tx = db.begin_tx();
        let bytes = b"test".to_vec();
        let hash = db.insert(&mut tx, 0, bytes);
        assert!(db.commit_tx(tx));
        bench.iter(move || {
            let mut tx = db.begin_tx();
            db.seek(&mut tx, 0, hash);
        });
    });
}

criterion_group!(scheduler, transactional_db);
criterion_main!(scheduler);
