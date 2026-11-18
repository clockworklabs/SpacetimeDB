use std::fs::OpenOptions;

use spacetimedb_sats::{product, AlgebraicType};
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_table::{
    indexes::SquashedOffset,
    tiered::{
        metrics::CacheTier, ByteBudget, ByteBudgetConfig, EvictionPolicy, TieredDatabaseStorage, TieredStorageConfig,
    },
};
use tempfile::tempdir;

use super::*;

mod versions;

#[test]
fn snapshot_evicts_other_tables_to_load_pages() -> anyhow::Result<()> {
    let mut config = TieredStorageConfig::new_for_test();
    config.resident_budget = ByteBudget::new(ByteBudgetConfig {
        tier: CacheTier::Memory,
        low_water_bytes: 0,
        soft_limit_bytes: size_of::<Page>() as u64,
        hard_limit_bytes: size_of::<Page>() as u64,
    })?;
    let pool = config.page_pool.clone();
    let storage = TieredDatabaseStorage::empty(config);
    let mut tables = [TableId(1), TableId(2)].map(|table_id| {
        let mut schema = TableSchema::from_product_type([AlgebraicType::U64].into());
        schema.table_id = table_id;
        Table::new(
            schema.into(),
            SquashedOffset::COMMITTED_STATE,
            storage.pages().clone(),
            EvictionPolicy::Memory,
        )
    });
    tables[0].insert(storage.blobs(), &product![11_u64])?;
    assert_eq!(tables[0].trim()?, size_of::<Page>() as u64);
    // The second table occupies the whole cache. Loading the first table
    // for the snapshot must evict from the second one.
    tables[1].insert(storage.blobs(), &product![22_u64])?;

    let tmp = tempdir()?;
    let repo = SnapshotRepository::open(SnapshotsPath::from_path_unchecked(tmp.path()), Identity::ZERO, 42)?;
    repo.create_snapshot(tables.iter_mut(), storage.blobs().scan(), 0)?
        .sync_all()?;
    let mut snapshot = repo.read_snapshot(0, &pool)?;
    assert_eq!(snapshot.tables.len(), 2);
    for (table, expected) in tables.iter().zip([11_u64, 22_u64]) {
        let pages = snapshot.tables.remove(&table.schema.table_id).unwrap();
        let mut restored = table.clone_structure(SquashedOffset::COMMITTED_STATE);
        // SAFETY: These pages came from the same table and retain its schema.
        unsafe { restored.set_pages(pages, storage.blobs())? };
        let mut rows = restored.scan_rows(storage.blobs());
        assert_eq!(rows.next().unwrap()?.to_product_value(), product![expected]);
        assert!(rows.next().is_none());
    }
    Ok(())
}

#[test]
fn listing_ignores_if_snapshot_file_is_missing() -> anyhow::Result<()> {
    let tmp = tempdir()?;

    let root = SnapshotsPath::from_path_unchecked(tmp.path());
    let repo = SnapshotRepository::open(root, Identity::ZERO, 42)?;
    for i in 0..10 {
        repo.snapshot_dir_path(i).create()?;
    }
    repo.snapshot_dir_path(5)
        .snapshot_file(5)
        .open_file(OpenOptions::new().write(true).create_new(true))
        .map(drop)?;

    assert_eq!(vec![5], repo.all_snapshots()?.collect::<Vec<_>>());

    Ok(())
}

#[test]
fn listing_ignores_if_lockfile_exists() -> anyhow::Result<()> {
    let tmp = tempdir()?;

    let root = SnapshotsPath::from_path_unchecked(tmp.path());
    let repo = SnapshotRepository::open(root, Identity::ZERO, 42)?;
    for i in 0..10 {
        let snapshot_dir = repo.snapshot_dir_path(i);
        snapshot_dir.create()?;
        snapshot_dir
            .snapshot_file(i)
            .open_file(OpenOptions::new().write(true).create_new(true))
            .map(drop)?;
    }
    let _lock = Lockfile::for_file(repo.snapshot_dir_path(5))?;

    let mut snapshots = repo.all_snapshots()?.collect::<Vec<_>>();
    snapshots.sort();
    assert_eq!(vec![0, 1, 2, 3, 4, 6, 7, 8, 9], snapshots);

    Ok(())
}
