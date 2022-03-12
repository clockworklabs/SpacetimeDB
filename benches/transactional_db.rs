use criterion::{criterion_group, criterion_main, Criterion};
use spacetimedb::transactional_db::TransactionalDB;

fn transactional_db(c: &mut Criterion) {
    c.bench_function("tx commit", |bench| {
        let mut db = TransactionalDB::new();
        bench.iter(move || {
            let mut tx = db.begin_tx();
            let bytes = b"test".to_vec();
            db.insert(&mut tx, bytes);
            assert!(db.commit_tx(tx));
        });
    });
}

criterion_group!(scheduler, transactional_db);
criterion_main!(scheduler);