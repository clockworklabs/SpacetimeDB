//! In-memory snapshot storage with deterministic fault injection.
//!
//! This is intentionally a semantic snapshot seam, not a filesystem facade. It
//! keeps DST snapshot bytes inside controlled memory storage, while still using
//! the same snapshot capture/restore shape as production.

use std::sync::Arc;

use spacetimedb_durability::TxOffset;
use spacetimedb_lib::Identity;
use spacetimedb_snapshot::{MemorySnapshotRepository, ReconstructedSnapshot, SnapshotError, SnapshotRepo};
use spacetimedb_table::{blob_store::BlobStore, page_pool::PagePool, table::Table};

use crate::{
    seed::DstSeed,
    sim::storage_faults::{
        is_injected_fault_text, StorageFaultConfig, StorageFaultController, StorageFaultDomain, StorageFaultKind,
        StorageFaultSummary,
    },
};

pub(crate) type SnapshotFaultConfig = StorageFaultConfig;

/// Returns true if `text` contains an error created by this snapshot fault layer.
pub(crate) fn is_injected_snapshot_error_text(text: &str) -> bool {
    is_injected_fault_text(StorageFaultDomain::Snapshot, text)
}

pub(crate) struct SnapshotRestoreRepo {
    pub(crate) store: Option<Arc<dyn SnapshotRepo>>,
    pub(crate) restored_snapshot_offset: Option<TxOffset>,
    pub(crate) latest_snapshot_offset: Option<TxOffset>,
}

/// In-memory snapshot repository wrapped with deterministic operation-level faults.
///
/// The bytes/pages are written and read by `spacetimedb-snapshot`; this wrapper
/// only decides whether a DST operation reaches that repository. That keeps
/// restore semantics aligned with production without requiring the
/// Tokio-backed `SnapshotWorker` or the host filesystem inside the simulator.
///
/// This is the intended boundary for the current DST target. It exercises
/// capture/restore behavior, retry classification, and replay correctness. It
/// does not model torn snapshot pages or byte-level corruption.
#[derive(Clone)]
pub(crate) struct BuggifiedSnapshotRepo {
    repo: Arc<MemorySnapshotRepository>,
    faults: StorageFaultController,
}

impl BuggifiedSnapshotRepo {
    pub(crate) fn new(config: SnapshotFaultConfig, seed: DstSeed) -> anyhow::Result<Self> {
        Ok(Self {
            repo: Arc::new(MemorySnapshotRepository::new(Identity::ZERO, 0)),
            faults: StorageFaultController::new(config, StorageFaultDomain::Snapshot, seed),
        })
    }

    pub(crate) fn enable_faults(&self) {
        self.faults.enable();
    }

    pub(crate) fn fault_summary(&self) -> StorageFaultSummary {
        self.faults.summary()
    }

    pub(crate) fn with_faults_suspended<T>(&self, f: impl FnOnce() -> T) -> T {
        self.faults.with_suspended(f)
    }

    pub(crate) fn latest_snapshot_unfaulted(&self) -> Result<Option<TxOffset>, String> {
        self.with_faults_suspended(|| {
            self.repo
                .latest_snapshot()
                .map_err(|err| format!("snapshot metadata read failed: {err}"))
        })
    }

    pub(crate) fn repo_for_restore(&self, durable_offset: Option<TxOffset>) -> Result<SnapshotRestoreRepo, String> {
        let latest_snapshot_offset = self.latest_snapshot_unfaulted()?;
        self.faults.maybe_latency();
        self.inject(StorageFaultKind::Metadata)?;
        let Some(durable_offset) = durable_offset else {
            return Ok(SnapshotRestoreRepo {
                store: None,
                restored_snapshot_offset: None,
                latest_snapshot_offset,
            });
        };
        let restored_snapshot_offset = self
            .repo
            .latest_snapshot_older_than(durable_offset)
            .map_err(|err| format!("snapshot metadata before restore failed: {err}"))?;
        if restored_snapshot_offset.is_none() {
            return Ok(SnapshotRestoreRepo {
                store: None,
                restored_snapshot_offset,
                latest_snapshot_offset,
            });
        }

        self.inject(StorageFaultKind::Open)?;
        self.inject(StorageFaultKind::Read)?;
        Ok(SnapshotRestoreRepo {
            store: Some(self.repo.clone()),
            restored_snapshot_offset,
            latest_snapshot_offset,
        })
    }

    fn inject(&self, kind: StorageFaultKind) -> Result<(), String> {
        self.faults.maybe_error(kind).map_err(|err| err.to_string())
    }
}

impl SnapshotRepo for BuggifiedSnapshotRepo {
    fn database_identity(&self) -> Identity {
        self.repo.database_identity()
    }

    fn capture_snapshot<'db>(
        &self,
        tables: &mut dyn Iterator<Item = &'db mut Table>,
        blobs: &'db dyn BlobStore,
        tx_offset: TxOffset,
    ) -> Result<TxOffset, SnapshotError> {
        self.faults.maybe_latency();
        self.faults
            .maybe_error(StorageFaultKind::Open)
            .map_err(SnapshotError::Io)?;
        self.faults
            .maybe_error(StorageFaultKind::Metadata)
            .map_err(SnapshotError::Io)?;
        self.faults
            .maybe_error(StorageFaultKind::Write)
            .map_err(SnapshotError::Io)?;
        self.faults
            .maybe_error(StorageFaultKind::Fsync)
            .map_err(SnapshotError::Io)?;
        self.repo.capture_snapshot(tables, blobs, tx_offset)
    }

    fn read_snapshot(&self, tx_offset: TxOffset, page_pool: &PagePool) -> Result<ReconstructedSnapshot, SnapshotError> {
        self.faults.maybe_latency();
        self.faults
            .maybe_error(StorageFaultKind::Open)
            .map_err(SnapshotError::Io)?;
        self.faults
            .maybe_error(StorageFaultKind::Read)
            .map_err(SnapshotError::Io)?;
        self.repo.read_snapshot(tx_offset, page_pool)
    }

    fn latest_snapshot_older_than(&self, upper_bound: TxOffset) -> Result<Option<TxOffset>, SnapshotError> {
        self.faults.maybe_latency();
        self.faults
            .maybe_error(StorageFaultKind::Metadata)
            .map_err(SnapshotError::Io)?;
        self.repo.latest_snapshot_older_than(upper_bound)
    }

    fn latest_snapshot(&self) -> Result<Option<TxOffset>, SnapshotError> {
        self.faults.maybe_latency();
        self.faults
            .maybe_error(StorageFaultKind::Metadata)
            .map_err(SnapshotError::Io)?;
        self.repo.latest_snapshot()
    }

    fn invalidate_newer_snapshots(&self, upper_bound: TxOffset) -> Result<(), SnapshotError> {
        self.faults.maybe_latency();
        self.faults
            .maybe_error(StorageFaultKind::Metadata)
            .map_err(SnapshotError::Io)?;
        self.repo.invalidate_newer_snapshots(upper_bound)
    }

    fn invalidate_snapshot(&self, tx_offset: TxOffset) -> Result<(), SnapshotError> {
        self.faults.maybe_latency();
        self.faults
            .maybe_error(StorageFaultKind::Metadata)
            .map_err(SnapshotError::Io)?;
        self.repo.invalidate_snapshot(tx_offset)
    }
}

#[cfg(test)]
mod tests {
    use crate::{config::CommitlogFaultProfile, seed::DstSeed};

    use super::*;

    fn no_faults() -> SnapshotFaultConfig {
        SnapshotFaultConfig::for_profile(CommitlogFaultProfile::Off)
    }

    fn always_metadata_error() -> SnapshotFaultConfig {
        SnapshotFaultConfig {
            enabled: true,
            metadata_error_prob: 1.0,
            ..SnapshotFaultConfig::for_profile(CommitlogFaultProfile::Default)
        }
    }

    #[test]
    fn repo_without_snapshots_is_not_used_for_restore() {
        let repo = BuggifiedSnapshotRepo::new(no_faults(), DstSeed(41)).unwrap();

        assert!(repo.repo_for_restore(Some(0)).unwrap().store.is_none());
    }

    #[test]
    fn injected_metadata_error_is_counted_and_recognizable() {
        let repo = BuggifiedSnapshotRepo::new(always_metadata_error(), DstSeed(42)).unwrap();
        repo.enable_faults();

        let err = match repo.repo_for_restore(Some(0)) {
            Ok(_) => panic!("expected injected snapshot metadata error"),
            Err(err) => err,
        };

        assert!(is_injected_snapshot_error_text(&err));
        assert_eq!(repo.fault_summary().metadata_error, 1);
    }

    #[test]
    fn suspended_faults_allow_restore_probe() {
        let repo = BuggifiedSnapshotRepo::new(always_metadata_error(), DstSeed(43)).unwrap();
        repo.enable_faults();

        let restore = repo.with_faults_suspended(|| repo.repo_for_restore(Some(0)));

        assert!(restore.unwrap().store.is_none());
        assert_eq!(repo.fault_summary().metadata_error, 0);
    }
}
