//! File Storage for one database.
//!
//! A file's bytes live in the database's [`FileStore`] under their BLAKE3 hash, and the file itself
//! is a row of `st_file`. File bytes never need the commitlog to be durable, with one exception:
//! a file that a reducer creates arrives inside a transaction, so its bytes travel in that
//! transaction as a row of `st_file_staged` until the host has persisted them.
//!
//! Clients that read with confirmed reads must never observe a transaction whose files are not yet
//! persisted in the store. [`FileGatedDurability`] enforces that by wrapping the database's
//! [`Durability`] and holding back the durable offset it reports until every file a transaction
//! references is persisted. Every confirmed-read path waits on that offset, so none of them change.

use crate::error::DBError;
use crate::persistence::Durability;
use crate::relational_db::{MutTx, RelationalDB, Txdata};
use bytes::Bytes;
use parking_lot::Mutex;
use spacetimedb_datastore::error::DatastoreError;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_datastore::system_tables::{
    StFileFields, StFileRow, StFileStagedFields, StFileStagedRow, TimestampViaI64, ST_FILE_ID, ST_FILE_STAGED_ID,
};
use spacetimedb_durability::{Close, DurableOffset, PreparedTx, TxOffset};
pub use spacetimedb_file_store::{ByteStream, FileStore, LocalFileStore, ReclaimStats, Spool, Spooled};
use spacetimedb_lib::{FileId, FileRef, Hash, Timestamp};
use spacetimedb_runtime::sync::{mpsc, watch};
use spacetimedb_runtime::Handle;
use spacetimedb_sats::{AlgebraicValue, ProductValue};
use spacetimedb_table::indexes::RowPointer;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// The most bytes a reducer may pass to `create`, because they travel through the commitlog.
pub const MAX_CREATED_FILE_BYTES: usize = 16 * 1024 * 1024;

/// How long reclamation spares an unreferenced object, measured from when it was last persisted.
///
/// Persisting an object that already exists refreshes its age, so this must exceed the longest time
/// between persisting a file and committing the `st_file` row that references it.
pub const RECLAIM_GRACE: Duration = Duration::from_secs(24 * 60 * 60);

/// File Storage state for one database.
pub struct DatabaseFiles {
    store: Arc<dyn FileStore>,
    runtime: Handle,
    uploads: Mutex<Uploads>,
    /// Persists hold this shared and reclamation holds it exclusively while it checks and deletes one
    /// object, so an object can't be deleted between being persisted again and being referenced.
    persist_lock: tokio::sync::RwLock<()>,
    /// Hashes of staged files that are now persisted, so their `st_file_staged` rows can go.
    staged_persisted: mpsc::UnboundedSender<Hash>,
    staged_persisted_rx: Mutex<Option<mpsc::UnboundedReceiver<Hash>>>,
    /// Becomes `true` once files staged before the database opened are persisted again.
    /// Until then, [`FileGatedDurability`] reports nothing durable.
    startup_settled: watch::Sender<bool>,
}

#[derive(Default)]
struct Uploads {
    /// Hashes known to be persisted in the store.
    ///
    /// A cache of positive answers. Reclamation removes each hash before it deletes the object.
    persisted: HashSet<Hash>,
    /// Staged files being persisted. Each receiver becomes `true` once its file is persisted.
    in_flight: HashMap<Hash, watch::Receiver<bool>>,
}

/// Removes an upload's `in_flight` entry however its task ends, so a failed upload can't
/// leave waiters on a channel that will never report.
struct InFlightGuard {
    files: Arc<DatabaseFiles>,
    hash: Hash,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.files.uploads.lock().in_flight.remove(&self.hash);
    }
}

impl DatabaseFiles {
    pub fn new(store: Arc<dyn FileStore>, runtime: Handle) -> Arc<Self> {
        let (staged_persisted, rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            store,
            runtime,
            uploads: Mutex::default(),
            persist_lock: tokio::sync::RwLock::new(()),
            staged_persisted,
            staged_persisted_rx: Mutex::new(Some(rx)),
            startup_settled: watch::channel(false).0,
        })
    }

    pub fn store(&self) -> &Arc<dyn FileStore> {
        &self.store
    }

    /// Stores a spooled upload, and returns once its bytes are durable.
    pub async fn persist(&self, spooled: Spooled) -> std::io::Result<()> {
        let hash = spooled.hash();
        let _persisting = self.persist_lock.read().await;
        self.store.persist(spooled).await?;
        self.note_persisted(hash);
        Ok(())
    }

    /// Records that `hash` is persisted in the store.
    fn note_persisted(&self, hash: Hash) {
        self.uploads.lock().persisted.insert(hash);
    }

    /// Whether `hash` is known to be persisted. `false` means unknown, not absent.
    pub fn is_known_persisted(&self, hash: &Hash) -> bool {
        self.uploads.lock().persisted.contains(hash)
    }

    /// Reports that files staged before the database opened are persisted again,
    /// which lets [`FileGatedDurability`] report transactions durable.
    pub fn settle_startup(&self) {
        self.startup_settled.send_replace(true);
    }

    /// Starts persisting the bytes of a staged file, unless that is already under way.
    ///
    /// The upload is retried until it succeeds. When it does, the file's `st_file_staged` row
    /// becomes eligible for deletion.
    pub fn persist_staged(self: &Arc<Self>, hash: Hash, bytes: Bytes) {
        let tx = {
            let mut uploads = self.uploads.lock();
            if uploads.in_flight.contains_key(&hash) {
                return;
            }
            let (tx, rx) = watch::channel(false);
            uploads.in_flight.insert(hash, rx);
            tx
        };
        let guard = InFlightGuard {
            files: self.clone(),
            hash,
        };
        self.runtime.spawn(async move {
            let this = guard.files.clone();
            let mut backoff = Duration::from_millis(50);
            loop {
                // Persisting is idempotent and refreshes the object's age even if it already exists,
                // which protects it from a reclamation pass that started before this transaction committed.
                let persisted = {
                    let _persisting = this.persist_lock.read().await;
                    let persisted = this.store.persist_bytes(hash, bytes.clone()).await;
                    if persisted.is_ok() {
                        this.note_persisted(hash);
                    }
                    persisted
                };
                match persisted {
                    Ok(()) => break,
                    Err(e) => log::warn!("failed to persist staged file {hash}, retrying in {backoff:?}: {e}"),
                }
                this.runtime.sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(5));
            }
            drop(guard);
            tx.send_replace(true);
            let _ = this.staged_persisted.send(hash);
        });
    }

    /// Resolves once `hash` is persisted in the store.
    ///
    /// A hash that no upload is persisting and that the store doesn't have is an invariant violation:
    /// some committed transaction references bytes that were never persisted. This logs it and keeps
    /// checking, so the transaction stays hidden from confirmed reads rather than being exposed.
    pub async fn wait_persisted(self: Arc<Self>, hash: Hash) {
        let mut backoff = Duration::from_millis(50);
        loop {
            let in_flight = {
                let uploads = self.uploads.lock();
                if uploads.persisted.contains(&hash) {
                    return;
                }
                uploads.in_flight.get(&hash).cloned()
            };
            if let Some(mut done) = in_flight
                && done.wait_for(|done| *done).await.is_ok()
            {
                continue;
            }
            // No upload is under way, or it ended without persisting: ask the store.
            match self.store.size(&hash).await {
                Ok(Some(_)) => {
                    self.note_persisted(hash);
                    return;
                }
                Ok(None) => {
                    log::error!("a committed transaction references file {hash}, which is not in the file store")
                }
                Err(e) => log::warn!("failed to check the file store for {hash}: {e}"),
            }
            self.runtime.sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(5));
        }
    }

    /// Deletes every object in the store that nothing can still need.
    ///
    /// `roots` must hold every hash that `st_file` rows name and that retained snapshots list.
    /// Staged files still being persisted are added here. Objects persisted within `grace` are spared,
    /// which covers uploads whose `st_file` row hasn't committed yet.
    pub async fn reclaim(&self, mut roots: HashSet<Hash>, grace: Duration) -> std::io::Result<ReclaimStats> {
        roots.extend(self.uploads.lock().in_flight.keys().copied());
        let cutoff = SystemTime::now().checked_sub(grace).unwrap_or(SystemTime::UNIX_EPOCH);
        let mut stats = ReclaimStats::default();
        for object in self.store.list().await? {
            if roots.contains(&object.hash) {
                continue;
            }
            if object.stored_at >= cutoff {
                stats.spared_young_objects += 1;
                continue;
            }
            // Check and delete with persists excluded, so an object persisted again in the meantime
            // is either seen as young here or written afresh after the delete.
            let _reclaiming = self.persist_lock.write().await;
            self.uploads.lock().persisted.remove(&object.hash);
            if self.store.delete_if_older_than(&object.hash, cutoff).await? {
                stats.deleted_objects += 1;
                stats.deleted_bytes += object.size;
            } else {
                stats.spared_young_objects += 1;
            }
        }
        Ok(stats)
    }
}

/// Generates a new file's ID and read secret.
///
/// The two are drawn independently from a cryptographically secure generator,
/// so knowing a file's ID reveals nothing about its secret.
pub fn mint_file_ref() -> FileRef {
    FileRef::new(FileId(rand::random()), rand::random())
}

/// Looks up the `st_file` row for `id`, whether or not the file has been deleted.
fn any_file_row(state: &impl StateView, id: FileId) -> Result<Option<(RowPointer, StFileRow)>, DatastoreError> {
    state
        .iter_by_col_eq(ST_FILE_ID, StFileFields::FileId, &AlgebraicValue::U128(id.0.into()))?
        .next()
        .map(|row| Ok((row.pointer(), StFileRow::try_from(row)?)))
        .transpose()
}

/// Looks up the live file `id` names.
pub fn live_file_row(state: &impl StateView, id: FileId) -> Result<Option<StFileRow>, DatastoreError> {
    Ok(any_file_row(state, id)?.map(|(_, row)| row).filter(StFileRow::is_live))
}

/// Looks up the live file `file_ref` names, if its read secret matches.
pub fn live_file_row_for_ref(state: &impl StateView, file_ref: FileRef) -> Result<Option<StFileRow>, DatastoreError> {
    Ok(servable_file_row_for_ref(state, file_ref)?.filter(StFileRow::is_live))
}

/// Looks up the file `file_ref` names, if its read secret matches, including a deleted file whose
/// record hasn't been purged yet.
///
/// Downloads use this, so deleting a file doesn't revoke downloads while history still retains it,
/// as the proposal specifies. To make deletion revoke downloads, use [`live_file_row_for_ref`] instead.
pub fn servable_file_row_for_ref(
    state: &impl StateView,
    file_ref: FileRef,
) -> Result<Option<StFileRow>, DatastoreError> {
    Ok(any_file_row(state, file_ref.id())?
        .map(|(_, row)| row)
        .filter(|row| row.read_secret == file_ref.read_secret()))
}

/// The bytes of a staged file, if they haven't been persisted and cleaned up yet.
pub fn staged_bytes(state: &impl StateView, hash: Hash) -> Result<Option<Box<[u8]>>, DatastoreError> {
    state
        .iter_by_col_eq(
            ST_FILE_STAGED_ID,
            StFileStagedFields::ContentHash,
            &hash.to_u256().into(),
        )?
        .next()
        .map(|row| StFileStagedRow::try_from(row).map(|row| row.bytes))
        .transpose()
}

/// Inserts a new file row.
pub fn insert_file_row(tx: &mut MutTx, row: &StFileRow) -> Result<(), DBError> {
    tx.insert_via_serialize_bsatn(ST_FILE_ID, row)?;
    Ok(())
}

/// Creates a file from `bytes` within `tx`, staging the bytes until they are persisted.
pub fn create_file(
    tx: &mut MutTx,
    bytes: &[u8],
    content_type: Option<String>,
    created_at: Timestamp,
) -> Result<FileRef, DBError> {
    let content_hash = spacetimedb_lib::file::content_hash(bytes);
    if staged_bytes(tx, content_hash)?.is_none() {
        tx.insert_via_serialize_bsatn(
            ST_FILE_STAGED_ID,
            &StFileStagedRow {
                content_hash,
                bytes: bytes.into(),
            },
        )?;
    }
    let file_ref = mint_file_ref();
    insert_file_row(
        tx,
        &StFileRow {
            file_id: file_ref.id(),
            read_secret: file_ref.read_secret(),
            content_hash,
            size: bytes.len() as u64,
            created_at: TimestampViaI64(created_at),
            content_type,
            deleted_at_offset: None,
        },
    )?;
    Ok(file_ref)
}

/// Creates a new file that shares `source`'s bytes and content type, or returns `None` if `source`
/// doesn't name a live file.
pub fn copy_file(tx: &mut MutTx, source: FileRef, created_at: Timestamp) -> Result<Option<FileRef>, DBError> {
    let Some(source) = live_file_row_for_ref(tx, source)? else {
        return Ok(None);
    };
    let file_ref = mint_file_ref();
    insert_file_row(
        tx,
        &StFileRow {
            file_id: file_ref.id(),
            read_secret: file_ref.read_secret(),
            created_at: TimestampViaI64(created_at),
            ..source
        },
    )?;
    Ok(Some(file_ref))
}

/// Deletes the live file `file_ref` names. Returns whether there was one.
///
/// The row stays, marked with this transaction's offset, so the bytes stay retained for as long as
/// history from before the deletion can be reverted to. See [`purge_deleted_files`].
pub fn delete_file(db: &RelationalDB, tx: &mut MutTx, file_ref: FileRef) -> Result<bool, DBError> {
    let Some((pointer, row)) = any_file_row(tx, file_ref.id())? else {
        return Ok(false);
    };
    if !row.is_live() || row.read_secret != file_ref.read_secret() {
        return Ok(false);
    }
    let deleted_at_offset = Some(tx.next_tx_offset());
    db.delete(tx, ST_FILE_ID, [pointer]);
    insert_file_row(
        tx,
        &StFileRow {
            deleted_at_offset,
            ..row
        },
    )?;
    Ok(true)
}

/// Removes the records of files deleted before `horizon`, the oldest transaction offset that a revert
/// can still reach. Returns how many it removed.
///
/// Every state from `horizon` on has these files deleted, so nothing can restore them. Once no retained
/// snapshot lists their hashes either, reclamation deletes their bytes.
///
/// A standalone host keeps its whole commitlog, which can be replayed from the start, so it never
/// purges. A host that truncates or archives history would call this with the start of what it keeps.
pub fn purge_deleted_files(db: &RelationalDB, tx: &mut MutTx, horizon: u64) -> Result<u32, DBError> {
    let mut purged = Vec::new();
    for row in tx.iter(ST_FILE_ID)? {
        let pointer = row.pointer();
        let row = StFileRow::try_from(row)?;
        if row.deleted_at_offset.is_some_and(|offset| offset < horizon) {
            purged.push(pointer);
        }
    }
    Ok(db.delete(tx, ST_FILE_ID, purged))
}

/// The file hashes and staged bytes that a transaction inserts.
fn inserted_files(txdata: &Txdata) -> (Vec<Hash>, Vec<(Hash, Bytes)>) {
    let mut hashes = Vec::new();
    let mut staged = Vec::new();
    let Some(mutations) = &txdata.mutations else {
        return (hashes, staged);
    };
    let hash_at = |row: &ProductValue, col: usize| row.elements.get(col)?.as_u256().map(|v| Hash::from_u256(**v));
    for ops in mutations.inserts.iter() {
        if ops.table_id == ST_FILE_ID {
            hashes.extend(
                ops.rowdata
                    .iter()
                    .filter_map(|row| hash_at(row, StFileFields::ContentHash as usize)),
            );
        } else if ops.table_id == ST_FILE_STAGED_ID {
            for row in ops.rowdata.iter() {
                let hash = hash_at(row, StFileStagedFields::ContentHash as usize);
                let bytes = row
                    .elements
                    .get(StFileStagedFields::Bytes as usize)
                    .and_then(|v| v.as_bytes());
                if let (Some(hash), Some(bytes)) = (hash, bytes) {
                    staged.push((hash, Bytes::copy_from_slice(bytes)));
                }
            }
        }
    }
    hashes.sort_unstable();
    hashes.dedup();
    (hashes, staged)
}

/// A [`Durability`] that reports a transaction durable only once the files it references are
/// persisted in the file store.
///
/// Transactions go to the inner durability immediately and in order. Only the reported offset is
/// held back: it is the inner durable offset, capped just below the first transaction that still
/// waits on a file. That preserves the prefix property every reader of the offset relies on.
///
/// The wrapper learns each transaction's files by wrapping its lazily prepared payload, which the
/// inner durability converts before it can report that transaction durable. Staged bytes start
/// persisting at the same moment, after the transaction has committed.
pub struct FileGatedDurability {
    inner: Arc<Durability>,
    files: Arc<DatabaseFiles>,
    gate: Arc<Gate>,
    runtime: Handle,
}

struct Gate {
    state: Mutex<GateState>,
}

struct GateState {
    /// What the inner durability reports.
    inner_durable: Option<TxOffset>,
    /// The newest transaction whose files have been registered.
    registered_through: Option<TxOffset>,
    /// Transactions still waiting on files, with the number of files each waits on.
    waiting: BTreeMap<TxOffset, usize>,
    /// `None` once the inner durability has exited, which tells waiters it is gone.
    published: Option<watch::Sender<Option<TxOffset>>>,
    /// The offset last reported durable, which outlives `published`.
    last_published: Option<TxOffset>,
    /// Whether files staged before the database opened are persisted again. Until they are,
    /// any transaction may reference one of them, so nothing is reported durable.
    startup_settled: bool,
    /// Set by `close`, after which the reported offset no longer moves.
    closed: bool,
}

impl Gate {
    fn publish(state: &mut GateState) {
        if state.closed || !state.startup_settled {
            return;
        }
        let mut offset = state.inner_durable.min(state.registered_through);
        if let Some(&first_waiting) = state.waiting.keys().next() {
            offset = offset.min(first_waiting.checked_sub(1));
        }
        if offset <= state.last_published {
            return;
        }
        state.last_published = offset;
        if let Some(published) = &state.published {
            published.send_replace(offset);
        }
    }

    fn file_persisted(&self, offset: TxOffset) {
        let mut state = self.state.lock();
        if let Some(remaining) = state.waiting.get_mut(&offset) {
            *remaining -= 1;
            if *remaining == 0 {
                state.waiting.remove(&offset);
            }
        }
        Self::publish(&mut state);
    }

    fn register(self: &Arc<Self>, files: &Arc<DatabaseFiles>, runtime: &Handle, offset: TxOffset, txdata: &Txdata) {
        let (hashes, staged) = inserted_files(txdata);
        for (hash, bytes) in staged {
            files.persist_staged(hash, bytes);
        }
        let pending: Vec<Hash> = hashes
            .into_iter()
            .filter(|hash| !files.is_known_persisted(hash))
            .collect();

        let mut state = self.state.lock();
        debug_assert!(
            state.registered_through.is_none_or(|through| offset > through),
            "transactions must reach the durability in offset order"
        );
        state.registered_through = Some(offset);
        if !pending.is_empty() {
            state.waiting.insert(offset, pending.len());
        }
        Self::publish(&mut state);
        drop(state);

        for hash in pending {
            let gate = self.clone();
            let files = files.clone();
            runtime.spawn(async move {
                files.wait_persisted(hash).await;
                gate.file_persisted(offset);
            });
        }
    }
}

impl FileGatedDurability {
    /// Wraps `inner`.
    ///
    /// Reports nothing durable until [`DatabaseFiles::settle_startup`], because the transactions
    /// replayed when the database opened may reference staged files that were never persisted.
    pub fn new(inner: Arc<Durability>, files: Arc<DatabaseFiles>, runtime: Handle) -> Self {
        let mut inner_offset = inner.durable_tx_offset();
        let initial = inner_offset.last_seen();
        let (published, _) = watch::channel(None);
        let gate = Arc::new(Gate {
            state: Mutex::new(GateState {
                inner_durable: initial,
                // Everything up to the inner durable offset was written before this wrapper existed,
                // and replay has already applied it.
                registered_through: initial,
                waiting: BTreeMap::new(),
                published: Some(published),
                last_published: None,
                startup_settled: false,
                closed: false,
            }),
        });

        let mut settled = files.startup_settled.subscribe();
        let settler = gate.clone();
        runtime.clone().spawn(async move {
            if settled.wait_for(|settled| *settled).await.is_ok() {
                let mut state = settler.state.lock();
                state.startup_settled = true;
                Gate::publish(&mut state);
            }
        });

        let watcher = gate.clone();
        runtime.clone().spawn(async move {
            loop {
                let next = watcher.state.lock().inner_durable.map_or(0, |offset| offset + 1);
                match inner_offset.wait_for(next).await {
                    Ok(offset) => {
                        let mut state = watcher.state.lock();
                        state.inner_durable = Some(offset);
                        Gate::publish(&mut state);
                    }
                    Err(_) => {
                        // Dropping the sender tells everyone waiting that the durability is gone.
                        watcher.state.lock().published = None;
                        return;
                    }
                }
            }
        });

        Self {
            inner,
            files,
            gate,
            runtime,
        }
    }
}

impl spacetimedb_durability::Durability for FileGatedDurability {
    type TxData = Txdata;

    fn append_tx(&self, tx: PreparedTx<Self::TxData>) {
        let gate = self.gate.clone();
        let files = self.files.clone();
        let runtime = self.runtime.clone();
        self.inner.append_tx(Box::new(move || {
            let tx = tx.into_transaction();
            gate.register(&files, &runtime, tx.offset, &tx.txdata);
            tx
        }));
    }

    fn durable_tx_offset(&self) -> DurableOffset {
        let state = self.gate.state.lock();
        match &state.published {
            Some(published) => published.subscribe().into(),
            // The inner durability exited. Hand out a handle that says so, and that still
            // remembers the last offset reported durable.
            None => watch::channel(state.last_published).1.into(),
        }
    }

    fn close(&self) -> Close {
        let close = self.inner.close();
        let gate = self.gate.clone();
        Box::pin(async move {
            let inner = close.await;
            let mut state = gate.state.lock();
            state.inner_durable = state.inner_durable.max(inner);
            Gate::publish(&mut state);
            state.closed = true;
            state.last_published
        })
    }
}

/// How often the maintenance task reclaims unreferenced file bytes.
pub const RECLAIM_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// The hashes reclamation must keep for `db`: those that `st_file` rows name, including deleted files
/// that haven't been purged, and those that retained snapshots list.
///
/// Because deletion only marks a row, a file inserted and deleted between two snapshots stays a root
/// until it is purged, so reverting to a point between the snapshots never needs reclaimed bytes.
pub fn reclamation_roots(db: &RelationalDB) -> Result<HashSet<Hash>, DBError> {
    let mut roots = db.with_read_only(Workload::Internal, |tx| -> Result<HashSet<Hash>, DBError> {
        tx.iter(ST_FILE_ID)?
            .map(|row| Ok(StFileRow::try_from(row)?.content_hash))
            .collect()
    })?;
    if let Some(repo) = db.snapshot_repo() {
        roots.extend(repo.retained_file_hashes().map_err(|e| DBError::Other(e.into()))?);
    }
    Ok(roots)
}

/// Deletes the file bytes that `db` can no longer need. See [`reclamation_roots`].
pub async fn reclaim_files(db: &Arc<RelationalDB>, grace: Duration, runtime: &Handle) -> anyhow::Result<ReclaimStats> {
    let files = db
        .files()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("this database doesn't support File Storage"))?;
    let roots = runtime
        .spawn_blocking({
            let db = db.clone();
            move || reclamation_roots(&db)
        })
        .await?;
    Ok(files.reclaim(roots, grace).await?)
}

/// Deletes the `st_file_staged` rows of `hashes`, whose bytes are now persisted.
fn delete_staged(db: &RelationalDB, hashes: Vec<Hash>) -> Result<(), DBError> {
    db.with_auto_commit(Workload::Internal, |tx| -> Result<(), DBError> {
        for hash in hashes {
            let pointer = tx
                .iter_by_col_eq(
                    ST_FILE_STAGED_ID,
                    StFileStagedFields::ContentHash,
                    &hash.to_u256().into(),
                )?
                .next()
                .map(|row| row.pointer());
            if let Some(pointer) = pointer {
                db.delete(tx, ST_FILE_STAGED_ID, [pointer]);
            }
        }
        Ok(())
    })
}

/// Runs the background work a database's files need:
///
/// - re-persisting files that were staged before the database opened, then letting the
///   [`FileGatedDurability`] report transactions durable;
/// - deleting `st_file_staged` rows once their bytes are persisted;
/// - reclaiming unreferenced bytes every [`RECLAIM_INTERVAL`].
///
/// Call once per database, after it is open. Until this runs, confirmed reads see nothing.
/// The tasks hold the database weakly, and stop once it is dropped.
pub fn spawn_file_maintenance(db: Arc<RelationalDB>, runtime: Handle) {
    let Some(files) = db.files().cloned() else {
        return;
    };
    let Some(mut persisted) = files.staged_persisted_rx.lock().take() else {
        return;
    };

    // Anything still staged was committed but maybe never persisted, for example because the host
    // crashed first. Its bytes are in the database, so persist them again before any transaction
    // is reported durable.
    let identity = db.database_identity();
    let settler = runtime.clone();
    let weak = Arc::downgrade(&db);
    runtime.spawn(async move {
        let Some(db) = weak.upgrade() else { return };
        let staged = settler
            .spawn_blocking(move || {
                db.with_read_only(Workload::Internal, |tx| -> Result<Vec<StFileStagedRow>, DBError> {
                    let rows = tx.iter(ST_FILE_STAGED_ID)?;
                    Ok(rows.map(StFileStagedRow::try_from).collect::<Result<_, _>>()?)
                })
            })
            .await;
        let staged = match staged {
            Ok(staged) => staged,
            Err(e) => {
                // Without knowing what was staged, nothing can be confirmed safely.
                log::error!("[{identity}] failed to read staged files, so no transaction will be confirmed: {e}");
                return;
            }
        };
        let hashes: Vec<Hash> = staged.iter().map(|row| row.content_hash).collect();
        for row in staged {
            files.persist_staged(row.content_hash, Bytes::from(row.bytes.into_vec()));
        }
        for hash in hashes {
            files.clone().wait_persisted(hash).await;
        }
        files.settle_startup();
    });

    let weak = Arc::downgrade(&db);
    let blocking = runtime.clone();
    // Ends when the database's `DatabaseFiles`, which holds the sender, is dropped.
    runtime.spawn(async move {
        let mut batch = Vec::new();
        while persisted.recv_many(&mut batch, 256).await > 0 {
            let hashes = std::mem::take(&mut batch);
            let Some(db) = weak.upgrade() else { return };
            let result = blocking.spawn_blocking(move || delete_staged(&db, hashes)).await;
            if let Err(e) = result {
                log::error!("failed to delete persisted staged files: {e}");
            }
        }
    });

    let weak = Arc::downgrade(&db);
    let identity = db.database_identity();
    drop(db);
    let reclaimer = runtime.clone();
    runtime.spawn(async move {
        loop {
            reclaimer.sleep(RECLAIM_INTERVAL).await;
            let Some(db) = weak.upgrade() else { return };
            match reclaim_files(&db, RECLAIM_GRACE, &reclaimer).await {
                Ok(stats) if stats.deleted_objects > 0 => log::info!(
                    "[{identity}] reclaimed {} unreferenced files ({} bytes)",
                    stats.deleted_objects,
                    stats.deleted_bytes
                ),
                Ok(_) => {}
                Err(e) => log::warn!("[{identity}] failed to reclaim files: {e:#}"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relational_db::tests_utils::TestDB;
    use async_trait::async_trait;
    use spacetimedb_commitlog::payload::txdata::{Mutations, Ops};
    use spacetimedb_datastore::traits::IsolationLevel;
    use spacetimedb_durability::{Durability as _, Transaction};
    use spacetimedb_file_store::{ByteStream, LocalFileStore, Spool, Spooled, StoredObject};
    use spacetimedb_lib::file::content_hash;
    use std::io;
    use std::ops::Range;
    use tokio::sync::Semaphore;

    /// An inner durability whose durable offset the test moves by hand.
    struct ManualDurability {
        durable: watch::Sender<Option<TxOffset>>,
    }

    impl ManualDurability {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                durable: watch::channel(None).0,
            })
        }

        fn make_durable(&self, offset: TxOffset) {
            self.durable.send_replace(Some(offset));
        }
    }

    impl spacetimedb_durability::Durability for ManualDurability {
        type TxData = Txdata;

        fn append_tx(&self, tx: PreparedTx<Txdata>) {
            // A real durability converts the payload before reporting it durable; do it right away.
            tx.into_transaction();
        }

        fn durable_tx_offset(&self) -> DurableOffset {
            self.durable.subscribe().into()
        }

        fn close(&self) -> Close {
            let offset = *self.durable.borrow();
            Box::pin(async move { offset })
        }
    }

    /// A store whose persists wait until the test releases them.
    struct HeldStore {
        inner: LocalFileStore,
        released: Semaphore,
    }

    #[async_trait]
    impl FileStore for HeldStore {
        async fn spool(&self) -> io::Result<Spool> {
            self.inner.spool().await
        }
        async fn persist(&self, spooled: Spooled) -> io::Result<()> {
            self.released.acquire().await.expect("never closed").forget();
            self.inner.persist(spooled).await
        }
        async fn size(&self, hash: &Hash) -> io::Result<Option<u64>> {
            self.inner.size(hash).await
        }
        async fn read(&self, hash: &Hash, range: Range<u64>) -> io::Result<ByteStream> {
            self.inner.read(hash, range).await
        }
        async fn list(&self) -> io::Result<Vec<StoredObject>> {
            self.inner.list().await
        }
        async fn delete(&self, hash: &Hash) -> io::Result<()> {
            self.inner.delete(hash).await
        }
    }

    fn file_row(contents: &[u8]) -> StFileRow {
        let file_ref = mint_file_ref();
        StFileRow {
            file_id: file_ref.id(),
            read_secret: file_ref.read_secret(),
            content_hash: content_hash(contents),
            size: contents.len() as u64,
            created_at: TimestampViaI64(Timestamp::UNIX_EPOCH),
            content_type: None,
            deleted_at_offset: None,
        }
    }

    /// A transaction that inserts `files`, staging the bytes of those in `staged`.
    fn tx(offset: TxOffset, files: &[&[u8]], staged: &[&[u8]]) -> PreparedTx<Txdata> {
        let mut inserts = Vec::new();
        if !files.is_empty() {
            let rows: Vec<ProductValue> = files.iter().map(|f| file_row(f).into()).collect();
            inserts.push(Ops {
                table_id: ST_FILE_ID,
                rowdata: rows.into(),
            });
        }
        if !staged.is_empty() {
            let rows: Vec<ProductValue> = staged
                .iter()
                .map(|bytes| {
                    StFileStagedRow {
                        content_hash: content_hash(bytes),
                        bytes: (*bytes).into(),
                    }
                    .into()
                })
                .collect();
            inserts.push(Ops {
                table_id: ST_FILE_STAGED_ID,
                rowdata: rows.into(),
            });
        }
        Box::new(Transaction {
            offset,
            txdata: Txdata {
                inputs: None,
                outputs: None,
                mutations: Some(Mutations {
                    inserts: inserts.into(),
                    deletes: [].into(),
                    truncates: [].into(),
                }),
            },
        })
    }

    /// Waits for the snapshot worker to finish a snapshot at or after `offset`.
    ///
    /// The worker snapshots on its own after the transaction at offset 1, which opening the
    /// database commits. Tests wait for that snapshot before taking their own, so the two don't race.
    fn wait_for_snapshot(stdb: &TestDB, rt: &tokio::runtime::Handle, offset: TxOffset) {
        let mut snapshots = stdb.subscribe_to_snapshots().expect("snapshots are enabled");
        rt.block_on(async {
            tokio::time::timeout(
                Duration::from_secs(10),
                snapshots.wait_for(|latest| latest.is_some_and(|latest| latest >= offset)),
            )
            .await
            .expect("the worker should snapshot")
            .expect("the worker is running");
        });
    }

    async fn settle() {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn confirmed_offset_waits_for_staged_files() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = Handle::tokio_current();
        let store = Arc::new(HeldStore {
            inner: LocalFileStore::open(dir.path()).unwrap(),
            released: Semaphore::new(0),
        });
        let files = DatabaseFiles::new(store.clone(), runtime.clone());
        files.settle_startup();
        let inner = ManualDurability::new();
        let gated = FileGatedDurability::new(inner.clone(), files.clone(), runtime);
        let mut confirmed = gated.durable_tx_offset();

        // A transaction without files is confirmed as soon as it is durable.
        gated.append_tx(tx(0, &[], &[]));
        inner.make_durable(0);
        confirmed.wait_for(0).await.unwrap();

        // A transaction that creates a file, and one after it without files.
        gated.append_tx(tx(1, &[b"hello"], &[b"hello"]));
        gated.append_tx(tx(2, &[], &[]));
        inner.make_durable(2);
        settle().await;
        // Both are durable in the log, but neither is confirmed until the file is persisted,
        // because confirmation is a prefix.
        assert_eq!(confirmed.last_seen(), Some(0));
        assert_eq!(store.size(&content_hash(b"hello")).await.unwrap(), None);

        store.released.add_permits(1);
        tokio::time::timeout(Duration::from_secs(5), confirmed.wait_for(2))
            .await
            .expect("the file should persist")
            .unwrap();
        assert_eq!(store.size(&content_hash(b"hello")).await.unwrap(), Some(5));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn confirmed_offset_never_passes_the_durable_offset() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = Handle::tokio_current();
        let store = Arc::new(LocalFileStore::open(dir.path()).unwrap());
        let files = DatabaseFiles::new(store, runtime.clone());
        files.settle_startup();
        let inner = ManualDurability::new();
        let gated = FileGatedDurability::new(inner.clone(), files, runtime);
        let confirmed = gated.durable_tx_offset();

        // The file persists quickly, but the log hasn't made the transaction durable yet.
        gated.append_tx(tx(0, &[b"x"], &[b"x"]));
        settle().await;
        assert_eq!(confirmed.last_seen(), None);
        inner.make_durable(0);
        settle().await;
        assert_eq!(confirmed.last_seen(), Some(0));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_missing_file_holds_confirmation_until_it_appears() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = Handle::tokio_current();
        let store = Arc::new(LocalFileStore::open(dir.path()).unwrap());
        let files = DatabaseFiles::new(store.clone(), runtime.clone());
        files.settle_startup();
        let inner = ManualDurability::new();
        let gated = FileGatedDurability::new(inner.clone(), files, runtime);
        let mut confirmed = gated.durable_tx_offset();

        // A row naming a file that nobody staged and the store doesn't have.
        gated.append_tx(tx(0, &[b"uploaded"], &[]));
        inner.make_durable(0);
        settle().await;
        assert_eq!(confirmed.last_seen(), None, "a dangling file must not be confirmed");

        // Once the bytes are in the store, as an upload would put them, the gate opens.
        store
            .persist_bytes(content_hash(b"uploaded"), Bytes::from_static(b"uploaded"))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), confirmed.wait_for(0))
            .await
            .expect("the gate should notice the file")
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn close_reports_the_confirmed_offset() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = Handle::tokio_current();
        let store = Arc::new(HeldStore {
            inner: LocalFileStore::open(dir.path()).unwrap(),
            released: Semaphore::new(0),
        });
        let files = DatabaseFiles::new(store.clone(), runtime.clone());
        files.settle_startup();
        let inner = ManualDurability::new();
        let gated = FileGatedDurability::new(inner.clone(), files, runtime);
        let confirmed = gated.durable_tx_offset();

        gated.append_tx(tx(0, &[], &[]));
        gated.append_tx(tx(1, &[b"held"], &[b"held"]));
        inner.make_durable(1);
        settle().await;
        assert_eq!(gated.close().await, Some(0));
        // The offset no longer moves after close, even when the file persists.
        store.released.add_permits(1);
        settle().await;
        assert_eq!(confirmed.last_seen(), Some(0));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn nothing_is_confirmed_until_startup_settles() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = Handle::tokio_current();
        let store = Arc::new(LocalFileStore::open(dir.path()).unwrap());
        let files = DatabaseFiles::new(store, runtime.clone());
        let inner = ManualDurability::new();
        // Transactions up to 4 were replayed when the database opened. Any of them may reference
        // a staged file that was never persisted.
        inner.make_durable(4);
        let gated = FileGatedDurability::new(inner.clone(), files.clone(), runtime);
        let mut confirmed = gated.durable_tx_offset();

        gated.append_tx(tx(5, &[], &[]));
        inner.make_durable(5);
        settle().await;
        assert_eq!(confirmed.last_seen(), None);

        files.settle_startup();
        tokio::time::timeout(Duration::from_secs(5), confirmed.wait_for(5))
            .await
            .expect("settling releases the hold")
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_last_confirmed_offset_survives_the_inner_durability() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = Handle::tokio_current();
        let store = Arc::new(LocalFileStore::open(dir.path()).unwrap());
        let files = DatabaseFiles::new(store, runtime.clone());
        files.settle_startup();
        let inner = ManualDurability::new();
        let gated = FileGatedDurability::new(inner.clone(), files, runtime);
        let mut confirmed = gated.durable_tx_offset();
        gated.append_tx(tx(0, &[], &[]));
        inner.make_durable(0);
        confirmed.wait_for(0).await.unwrap();

        // The inner durability exits, as `Local` does when its actor stops.
        drop(gated);
        drop(inner);
        settle().await;
        assert_eq!(confirmed.last_seen(), Some(0));
        assert!(confirmed.get().is_err(), "waiters learn that the durability is gone");
    }

    #[test]
    fn created_files_are_persisted_cleaned_up_and_snapshotted() -> anyhow::Result<()> {
        let stdb = TestDB::durable_with_files()?;
        let rt = stdb.runtime().expect("durable").clone();
        let files = stdb.files().expect("files are enabled").clone();
        wait_for_snapshot(&stdb, &rt, 1);

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        let file_ref = create_file(&mut tx, b"hello, files", Some("text/plain".into()), Timestamp::now())?;
        let (offset, ..) = stdb.commit_tx(tx)?.expect("the transaction consumes an offset");

        // Confirmed reads see the transaction only once its file is persisted.
        rt.block_on(async {
            let mut confirmed = stdb.durable_tx_offset().expect("durable");
            tokio::time::timeout(Duration::from_secs(10), confirmed.wait_for(offset)).await
        })??;
        let hash = content_hash(b"hello, files");
        assert_eq!(rt.block_on(files.store().size(&hash))?, Some(12));

        // The staged bytes are deleted once they're persisted.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while stdb
            .with_read_only(Workload::ForTests, |tx| staged_bytes(tx, hash))?
            .is_some()
        {
            assert!(std::time::Instant::now() < deadline, "staged row was never cleaned up");
            std::thread::sleep(Duration::from_millis(20));
        }

        // The row is found by its reference, and only with the right secret.
        let row = stdb.with_read_only(Workload::ForTests, |tx| live_file_row_for_ref(tx, file_ref))?;
        let row = row.expect("the file exists");
        assert_eq!(row.content_hash, hash);
        assert_eq!(row.content_type.as_deref(), Some("text/plain"));
        let forged = FileRef::new(file_ref.id(), file_ref.read_secret() ^ 1);
        assert!(stdb
            .with_read_only(Workload::ForTests, |tx| servable_file_row_for_ref(tx, forged))?
            .is_none());

        // A snapshot lists the file, so reclamation keeps it while the snapshot is retained.
        let repo = stdb.snapshot_repo().expect("snapshots are enabled");
        assert!(!repo.retained_file_hashes()?.contains(&hash));
        stdb.take_snapshot(&*repo)?.expect("a snapshot is taken");
        assert!(repo.retained_file_hashes()?.contains(&hash));
        Ok(())
    }

    #[test]
    fn copying_shares_bytes_and_deleting_keeps_them() -> anyhow::Result<()> {
        let stdb = TestDB::durable_with_files()?;
        let now = Timestamp::now();
        let (original, copy) = stdb.with_auto_commit(Workload::ForTests, |tx| -> Result<_, DBError> {
            let original = create_file(tx, b"shared", None, now)?;
            let copy = copy_file(tx, original, now)?.expect("the original exists");
            Ok((original, copy))
        })?;
        assert_ne!(original.id(), copy.id());

        let deleted = stdb.with_auto_commit(Workload::ForTests, |tx| delete_file(&stdb, tx, original))?;
        assert!(deleted);
        let (live, servable, copy_row) = stdb.with_read_only(Workload::ForTests, |tx| -> Result<_, DBError> {
            Ok((
                live_file_row_for_ref(tx, original)?,
                servable_file_row_for_ref(tx, original)?,
                live_file_row_for_ref(tx, copy)?,
            ))
        })?;
        // The original is gone to modules, but its record stays until it's purged, so it still downloads.
        assert!(live.is_none());
        assert!(servable.expect("the record stays").deleted_at_offset.is_some());
        assert_eq!(
            copy_row.expect("the copy survives").content_hash,
            content_hash(b"shared")
        );
        // Deleting it again finds nothing live.
        assert!(!stdb.with_auto_commit(Workload::ForTests, |tx| delete_file(&stdb, tx, original))?);
        // Nor can a deleted file be copied.
        assert!(stdb
            .with_auto_commit(Workload::ForTests, |tx| copy_file(tx, original, now))?
            .is_none());

        // Deleting with the wrong secret does nothing.
        let forged = FileRef::new(copy.id(), copy.read_secret() ^ 1);
        assert!(!stdb.with_auto_commit(Workload::ForTests, |tx| delete_file(&stdb, tx, forged))?);
        Ok(())
    }

    #[test]
    fn deleted_files_stay_until_purged_and_snapshots_keep_their_bytes() -> anyhow::Result<()> {
        let stdb = TestDB::durable_with_files()?;
        let rt = stdb.runtime().expect("durable").clone();
        let runtime = Handle::tokio(rt.clone());
        let files = stdb.files().expect("files are enabled").clone();
        let commit_and_confirm = |f: &dyn Fn(&mut MutTx) -> Result<(), DBError>| -> anyhow::Result<TxOffset> {
            let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
            f(&mut tx)?;
            let (offset, ..) = stdb.commit_tx(tx)?.expect("consumes an offset");
            rt.block_on(async { stdb.durable_tx_offset().expect("durable").wait_for(offset).await })?;
            Ok(offset)
        };
        let size = |contents: &[u8]| rt.block_on(files.store().size(&content_hash(contents))).unwrap();
        let now = Timestamp::now();
        let created = std::cell::RefCell::new(Vec::new());
        wait_for_snapshot(&stdb, &rt, 1);

        // `live` stays. `snapshotted` is deleted after a snapshot captures it.
        // `deleted` is created and deleted between snapshots. `orphan` is an upload nothing recorded.
        commit_and_confirm(&|tx| {
            created.borrow_mut().push(create_file(tx, b"live", None, now)?);
            created.borrow_mut().push(create_file(tx, b"snapshotted", None, now)?);
            Ok(())
        })?;
        stdb.take_snapshot(&*stdb.snapshot_repo().expect("snapshots are enabled"))?
            .expect("a snapshot is taken");
        commit_and_confirm(&|tx| {
            created.borrow_mut().push(create_file(tx, b"deleted", None, now)?);
            Ok(())
        })?;
        let [_live, snapshotted, deleted] = created.borrow()[..] else {
            unreachable!()
        };
        let deleted_at = commit_and_confirm(&|tx| {
            assert!(delete_file(&stdb, tx, snapshotted)?);
            assert!(delete_file(&stdb, tx, deleted)?);
            Ok(())
        })?;
        rt.block_on(
            files
                .store()
                .persist_bytes(content_hash(b"orphan"), Bytes::from_static(b"orphan")),
        )?;

        // Deleted files are still roots, because reverting to before their deletion restores them.
        let stats = rt.block_on(reclaim_files(&stdb, Duration::ZERO, &runtime))?;
        assert_eq!(stats.deleted_objects, 1);
        assert_eq!(size(b"orphan"), None);
        assert_eq!(size(b"deleted"), Some(7));

        // History from before the deletion is gone, so the records go too.
        commit_and_confirm(&|tx| {
            assert_eq!(purge_deleted_files(&stdb, tx, deleted_at)?, 0, "not before the horizon");
            assert_eq!(purge_deleted_files(&stdb, tx, deleted_at + 1)?, 2);
            Ok(())
        })?;
        let stats = rt.block_on(reclaim_files(&stdb, Duration::ZERO, &runtime))?;
        assert_eq!(stats.deleted_objects, 1);
        assert_eq!(size(b"live"), Some(4));
        assert_eq!(size(b"snapshotted"), Some(11), "a retained snapshot still lists it");
        assert_eq!(size(b"deleted"), None);
        Ok(())
    }
}
