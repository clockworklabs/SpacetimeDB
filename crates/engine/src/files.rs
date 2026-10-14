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
    IdentityViaU256, StFileFields, StFileRow, StFileStagedFields, StFileStagedRow, StUploadFields, StUploadRow,
    TimestampViaI64, ST_FILE_ID, ST_FILE_STAGED_ID, ST_UPLOAD_ID,
};
use spacetimedb_durability::{Close, DurableOffset, PreparedTx, TxOffset};
pub use spacetimedb_file_store::{ByteStream, FileStore, LocalFileStore, ReclaimStats, Spool, Spooled};
use spacetimedb_lib::{FileId, FileRef, Hash, Identity, Timestamp};
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

/// The longest `Idempotency-Key` an upload may carry.
pub const MAX_IDEMPOTENCY_KEY_LEN: usize = 255;

/// How long a completed upload is remembered by its idempotency key.
pub const UPLOAD_KEY_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

/// Whether `key` can name an upload: nonempty, at most [`MAX_IDEMPOTENCY_KEY_LEN`] bytes,
/// and only visible ASCII.
pub fn is_valid_idempotency_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= MAX_IDEMPOTENCY_KEY_LEN && key.bytes().all(|b| b.is_ascii_graphic())
}

/// File Storage state for one database.
pub struct DatabaseFiles {
    store: Arc<dyn FileStore>,
    runtime: Handle,
    uploads: Mutex<Uploads>,
    /// Persists hold their hash's stripe shared and reclamation holds it exclusively while it checks
    /// and deletes one object, so an object can't be deleted between being persisted again and being
    /// referenced. Striping keeps one slow persist from stalling reclamation of unrelated objects,
    /// and reclamation from stalling unrelated persists.
    persist_locks: Box<[tokio::sync::RwLock<()>]>,
    /// Hashes of staged files that are now persisted, so their `st_file_staged` rows can go.
    staged_persisted: mpsc::UnboundedSender<Hash>,
    staged_persisted_rx: Mutex<Option<mpsc::UnboundedReceiver<Hash>>>,
    /// Becomes `true` once files staged before the database opened are persisted again.
    /// Until then, [`FileGatedDurability`] reports nothing durable.
    startup_settled: watch::Sender<bool>,
    /// The `(sender, idempotency key)` of each keyed upload in progress.
    keyed_uploads: Mutex<HashSet<(Identity, String)>>,
    /// Limits how many uploads may be in progress at once.
    upload_slots: Arc<tokio::sync::Semaphore>,
    /// Becomes `true` when the database closes, which stops background uploads and waits.
    shut_down: watch::Sender<bool>,
}

/// The most uploads a database accepts at once.
pub const MAX_UPLOADS_IN_PROGRESS: usize = 256;

const PERSIST_LOCK_STRIPES: usize = 64;

/// Marks a keyed upload as in progress until dropped.
pub struct KeyedUpload {
    files: Arc<DatabaseFiles>,
    key: (Identity, String),
}

impl Drop for KeyedUpload {
    fn drop(&mut self) {
        self.files.keyed_uploads.lock().remove(&self.key);
    }
}

#[derive(Default)]
struct Uploads {
    /// Hashes known to be persisted in the store.
    ///
    /// A cache of positive answers. Reclamation removes each hash before it deletes the object.
    persisted: HashSet<Hash>,
    /// Staged files being persisted. Each receiver becomes `true` once its file is persisted.
    in_flight: HashMap<Hash, watch::Receiver<bool>>,
    /// Uploads whose bytes are, or are about to be, persisted but whose `st_file` rows haven't
    /// committed, counted by hash. See [`PendingUpload`].
    pending_uploads: HashMap<Hash, u32>,
}

/// Keeps an upload's object from being reclaimed until its `st_file` row commits or the upload is
/// abandoned, however long that takes.
///
/// Between its bytes being persisted and its row committing, nothing names an upload's object, so
/// without this only the grace period protects it, and a stall longer than that would leave a
/// committed row naming bytes that reclamation deleted. Hold the guard until the row commits.
pub struct PendingUpload {
    files: Arc<DatabaseFiles>,
    hash: Hash,
}

impl PendingUpload {
    fn new(files: Arc<DatabaseFiles>, hash: Hash) -> Self {
        *files.uploads.lock().pending_uploads.entry(hash).or_insert(0) += 1;
        Self { files, hash }
    }
}

impl Drop for PendingUpload {
    fn drop(&mut self) {
        let mut uploads = self.files.uploads.lock();
        let count = uploads
            .pending_uploads
            .get_mut(&self.hash)
            .expect("a pending upload is counted until it's dropped");
        *count -= 1;
        if *count == 0 {
            uploads.pending_uploads.remove(&self.hash);
        }
    }
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
            persist_locks: (0..PERSIST_LOCK_STRIPES)
                .map(|_| tokio::sync::RwLock::new(()))
                .collect(),
            staged_persisted,
            staged_persisted_rx: Mutex::new(Some(rx)),
            startup_settled: watch::channel(false).0,
            keyed_uploads: Mutex::default(),
            upload_slots: Arc::new(tokio::sync::Semaphore::new(MAX_UPLOADS_IN_PROGRESS)),
            shut_down: watch::channel(false).0,
        })
    }

    /// Reserves one of the [`MAX_UPLOADS_IN_PROGRESS`] upload slots, or returns `None` if they are all
    /// taken. The slot is released when the permit is dropped.
    pub fn try_begin_upload(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        self.upload_slots.clone().try_acquire_owned().ok()
    }

    /// Stops background uploads and waits, because the database is closing.
    ///
    /// Staged files whose uploads stop are persisted again when the database next opens.
    pub fn shut_down(&self) {
        self.shut_down.send_replace(true);
    }

    /// Sleeps for `duration`, and returns `false` if the database closed in the meantime.
    async fn sleep_unless_shut_down(&self, duration: Duration) -> bool {
        let mut shut_down = self.shut_down.subscribe();
        tokio::select! {
            () = self.runtime.sleep(duration) => {}
            _ = shut_down.wait_for(|shut_down| *shut_down) => return false,
        }
        !*self.shut_down.borrow()
    }

    /// Marks the upload `key` from `sender` as in progress, or returns `None` if it already is,
    /// in which case the caller should refuse the retry until the first attempt finishes.
    pub fn begin_keyed_upload(self: &Arc<Self>, sender: Identity, key: &str) -> Option<KeyedUpload> {
        let key = (sender, key.to_owned());
        self.keyed_uploads.lock().insert(key.clone()).then(|| KeyedUpload {
            files: self.clone(),
            key,
        })
    }

    pub fn store(&self) -> &Arc<dyn FileStore> {
        &self.store
    }

    fn persist_lock(&self, hash: &Hash) -> &tokio::sync::RwLock<()> {
        &self.persist_locks[hash.data[0] as usize % self.persist_locks.len()]
    }

    /// Stores a spooled upload, and returns once its bytes are durable.
    ///
    /// The upload counts as pending, and its object can't be reclaimed, until the returned guard is
    /// dropped. Hold it until the upload's `st_file` row has committed, or the upload is abandoned.
    pub async fn persist(self: &Arc<Self>, spooled: Spooled) -> std::io::Result<PendingUpload> {
        let hash = spooled.hash();
        // Count the upload before persisting, so a reclamation pass that checks the hash from now on
        // spares the object, and one that is already deleting it finishes before the persist starts.
        let pending = PendingUpload::new(self.clone(), hash);
        let _persisting = self.persist_lock(&hash).read().await;
        self.store.persist(spooled).await?;
        self.note_persisted(hash);
        Ok(pending)
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
                if *this.shut_down.borrow() {
                    return;
                }
                // Persisting is idempotent and refreshes the object's age even if it already exists,
                // which protects it from a reclamation pass that started before this transaction committed.
                let persisted = {
                    let _persisting = this.persist_lock(&hash).read().await;
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
                if !this.sleep_unless_shut_down(backoff).await {
                    return;
                }
                backoff = (backoff * 2).min(Duration::from_secs(5));
            }
            drop(guard);
            tx.send_replace(true);
            let _ = this.staged_persisted.send(hash);
        });
    }

    /// Resolves to `true` once `hash` is persisted in the store, or to `false` if the database
    /// closes first.
    ///
    /// A hash that no upload is persisting and that the store doesn't have is an invariant violation:
    /// some committed transaction references bytes that were never persisted. This logs it and keeps
    /// checking, so the transaction stays hidden from confirmed reads rather than being exposed.
    pub async fn wait_persisted(self: Arc<Self>, hash: Hash) -> bool {
        let mut backoff = Duration::from_millis(50);
        let mut shut_down = self.shut_down.subscribe();
        loop {
            if *shut_down.borrow() {
                return false;
            }
            let in_flight = {
                let uploads = self.uploads.lock();
                match uploads.in_flight.get(&hash) {
                    // Wait for a persist under way even if the hash is known to be persisted: it is what
                    // refreshes the object's age, and the object may be about to be reclaimed.
                    Some(done) => Some(done.clone()),
                    None if uploads.persisted.contains(&hash) => return true,
                    None => None,
                }
            };
            if let Some(mut done) = in_flight {
                let finished = tokio::select! {
                    finished = done.wait_for(|done| *done) => finished.is_ok(),
                    _ = shut_down.wait_for(|shut_down| *shut_down) => return false,
                };
                if finished {
                    continue;
                }
            }
            // No upload is under way, or it ended without persisting: ask the store.
            match self.store.size(&hash).await {
                Ok(Some(_)) => {
                    self.note_persisted(hash);
                    return true;
                }
                Ok(None) => {
                    log::error!("a committed transaction references file {hash}, which is not in the file store")
                }
                Err(e) => log::warn!("failed to check the file store for {hash}: {e}"),
            }
            if !self.sleep_unless_shut_down(backoff).await {
                return false;
            }
            backoff = (backoff * 2).min(Duration::from_secs(5));
        }
    }

    /// Deletes every object in the store that nothing can still need.
    ///
    /// `roots` must hold every hash that `st_file` rows name and that retained snapshots list.
    /// Staged files still being persisted are added here. Objects persisted within `grace` are spared,
    /// which covers uploads whose `st_file` row hasn't committed yet.
    ///
    /// `roots` can be stale by the time an object is deleted, since transactions keep committing.
    /// So each object is checked again just before it is deleted, with `still_named`, which reports
    /// whether the current `st_file` names a hash.
    pub async fn reclaim(
        &self,
        mut roots: HashSet<Hash>,
        grace: Duration,
        still_named: impl Fn(&Hash) -> bool,
    ) -> std::io::Result<ReclaimStats> {
        {
            let uploads = self.uploads.lock();
            roots.extend(uploads.in_flight.keys().copied());
            roots.extend(uploads.pending_uploads.keys().copied());
        }
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
            let _reclaiming = self.persist_lock(&object.hash).write().await;
            // Forget the hash first, so a transaction that names it from now on waits for a persist,
            // which can't start until this delete is done.
            let about_to_be_named = {
                let mut uploads = self.uploads.lock();
                uploads.persisted.remove(&object.hash);
                uploads.in_flight.contains_key(&object.hash) || uploads.pending_uploads.contains_key(&object.hash)
            };
            // A transaction that committed after `roots` were gathered may name the object, and an
            // upload that started since may be about to.
            if about_to_be_named || still_named(&object.hash) {
                stats.spared_named_objects += 1;
                continue;
            }
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

/// Looks up the file `id` names, including a deleted file whose record hasn't been purged.
///
/// This doesn't check a read secret, so only the host may use it, for files it already knows about.
pub fn servable_file_row(state: &impl StateView, id: FileId) -> Result<Option<StFileRow>, DatastoreError> {
    Ok(any_file_row(state, id)?.map(|(_, row)| row))
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
    Ok(any_file_row(state, file_ref.id_unchecked())?
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

/// The record of the upload `sender` completed with `key`, whether or not it has expired.
fn upload_record(
    state: &impl StateView,
    sender: Identity,
    key: &str,
) -> Result<Option<(RowPointer, StUploadRow)>, DBError> {
    let key = AlgebraicValue::product([IdentityViaU256(sender).into(), AlgebraicValue::String(key.into())]);
    let row = state
        .iter_by_col_eq(
            ST_UPLOAD_ID,
            spacetimedb_primitives::col_list![StUploadFields::Sender, StUploadFields::IdempotencyKey],
            &key,
        )?
        .next()
        .map(|row| Ok::<_, DBError>((row.pointer(), StUploadRow::try_from(row)?)))
        .transpose()?;
    Ok(row)
}

fn is_expired(row: &StUploadRow, now: Timestamp) -> bool {
    now.checked_sub_duration(UPLOAD_KEY_RETENTION)
        .is_some_and(|cutoff| row.completed_at.0 < cutoff)
}

/// The upload that `sender` completed with the idempotency key `key`, unless it has expired.
pub fn completed_upload(
    state: &impl StateView,
    sender: Identity,
    key: &str,
    now: Timestamp,
) -> Result<Option<StUploadRow>, DBError> {
    Ok(upload_record(state, sender, key)?
        .map(|(_, row)| row)
        .filter(|row| !is_expired(row, now)))
}

/// Remembers a completed upload by its idempotency key, replacing an expired record of the same key.
///
/// If the key already names an upload that hasn't expired, returns that upload's file and records
/// nothing: the caller lost a race with another attempt of the same upload.
pub fn record_upload(
    db: &RelationalDB,
    tx: &mut MutTx,
    row: &StUploadRow,
    now: Timestamp,
) -> Result<Option<FileId>, DBError> {
    if let Some((pointer, existing)) = upload_record(tx, row.sender.0, &row.idempotency_key)? {
        if !is_expired(&existing, now) {
            return Ok(Some(existing.file_id));
        }
        db.delete(tx, ST_UPLOAD_ID, [pointer]);
    }
    tx.insert_via_serialize_bsatn(ST_UPLOAD_ID, row)?;
    Ok(None)
}

/// Forgets uploads completed more than `retention` before `now`. Returns how many it forgot.
pub fn expire_upload_records(
    db: &RelationalDB,
    tx: &mut MutTx,
    now: Timestamp,
    retention: Duration,
) -> Result<u32, DBError> {
    let cutoff = now.checked_sub_duration(retention).unwrap_or(Timestamp::UNIX_EPOCH);
    let mut expired = Vec::new();
    for row in tx.iter(ST_UPLOAD_ID)? {
        let pointer = row.pointer();
        if StUploadRow::try_from(row)?.completed_at.0 < cutoff {
            expired.push(pointer);
        }
    }
    Ok(db.delete(tx, ST_UPLOAD_ID, expired))
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
            file_id: file_ref.id_unchecked(),
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
/// isn't a live file.
pub fn copy_file(tx: &mut MutTx, source: FileId, created_at: Timestamp) -> Result<Option<FileRef>, DBError> {
    let Some(source) = live_file_row(tx, source)? else {
        return Ok(None);
    };
    let file_ref = mint_file_ref();
    insert_file_row(
        tx,
        &StFileRow {
            file_id: file_ref.id_unchecked(),
            read_secret: file_ref.read_secret(),
            created_at: TimestampViaI64(created_at),
            ..source
        },
    )?;
    Ok(Some(file_ref))
}

/// Deletes the live file `id`. Returns whether there was one.
///
/// The row stays, marked with this transaction's offset, so the bytes stay retained for as long as
/// history from before the deletion can be reverted to. See [`purge_deleted_files`].
pub fn delete_file(db: &RelationalDB, tx: &mut MutTx, id: FileId) -> Result<bool, DBError> {
    let Some((pointer, row)) = any_file_row(tx, id)? else {
        return Ok(false);
    };
    if !row.is_live() {
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
        // A staged file waits for its own persist even when the store is known to have its hash,
        // because a reclamation pass that began before this transaction committed may be about to
        // delete the object, and the persist is what puts it back or refreshes its age.
        let mut pending: Vec<Hash> = Vec::new();
        for (hash, bytes) in staged {
            files.persist_staged(hash, bytes);
            if !pending.contains(&hash) {
                pending.push(hash);
            }
        }
        for hash in hashes {
            if !pending.contains(&hash) && !files.is_known_persisted(&hash) {
                pending.push(hash);
            }
        }

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
                if files.wait_persisted(hash).await {
                    gate.file_persisted(offset);
                }
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
    Ok(files.reclaim(roots, grace, |hash| file_is_named(db, hash)).await?)
}

/// Whether any `st_file` row in `db`'s committed state names `hash`, deleted rows included.
///
/// Errs on the side of `true`, so that a failed lookup spares the object.
fn file_is_named(db: &RelationalDB, hash: &Hash) -> bool {
    let named = db.with_read_only(Workload::Internal, |tx| -> Result<bool, DBError> {
        let key: AlgebraicValue = hash.to_u256().into();
        let mut rows = tx.iter_by_col_eq(ST_FILE_ID, StFileFields::ContentHash, &key)?;
        Ok(rows.next().is_some())
    });
    named.unwrap_or_else(|e| {
        log::warn!("failed to check whether {hash} is still named, so sparing it: {e}");
        true
    })
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
        // Without knowing what was staged, nothing can be confirmed safely, so keep trying.
        let mut backoff = Duration::from_millis(50);
        let staged = loop {
            let Some(db) = weak.upgrade() else { return };
            let staged = settler
                .spawn_blocking(move || {
                    db.with_read_only(Workload::Internal, |tx| -> Result<Vec<StFileStagedRow>, DBError> {
                        let rows = tx.iter(ST_FILE_STAGED_ID)?;
                        Ok(rows.map(StFileStagedRow::try_from).collect::<Result<_, _>>()?)
                    })
                })
                .await;
            match staged {
                Ok(staged) => break staged,
                Err(e) => log::error!("[{identity}] failed to read staged files, retrying in {backoff:?}: {e}"),
            }
            if !files.sleep_unless_shut_down(backoff).await {
                return;
            }
            backoff = (backoff * 2).min(Duration::from_secs(5));
        };
        let hashes: Vec<Hash> = staged.iter().map(|row| row.content_hash).collect();
        for row in staged {
            files.persist_staged(row.content_hash, Bytes::from(row.bytes.into_vec()));
        }
        for hash in hashes {
            if !files.clone().wait_persisted(hash).await {
                return;
            }
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
        let mut opened = true;
        loop {
            // Expire upload records as soon as the database opens, since a host that restarts often
            // might otherwise never reach the first interval. Reclaim only on the interval.
            let just_opened = std::mem::take(&mut opened);
            if !just_opened {
                reclaimer.sleep(RECLAIM_INTERVAL).await;
            }
            let Some(db) = weak.upgrade() else { return };
            let expired = reclaimer
                .spawn_blocking({
                    let db = db.clone();
                    move || {
                        db.with_auto_commit(Workload::Internal, |tx| {
                            expire_upload_records(&db, tx, Timestamp::now(), UPLOAD_KEY_RETENTION)
                        })
                    }
                })
                .await;
            if let Err(e) = expired {
                log::warn!("[{identity}] failed to expire upload records: {e}");
            }
            if just_opened {
                continue;
            }
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
            file_id: file_ref.id_unchecked(),
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
    async fn shutting_down_stops_uploads_without_confirming_them() {
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
        let confirmed = gated.durable_tx_offset();

        gated.append_tx(tx(0, &[b"never"], &[b"never"]));
        inner.make_durable(0);
        let waiting = tokio::spawn(files.clone().wait_persisted(content_hash(b"never")));
        settle().await;
        files.shut_down();
        let persisted = tokio::time::timeout(Duration::from_secs(5), waiting)
            .await
            .expect("shutting down ends the wait")
            .unwrap();
        assert!(!persisted);
        settle().await;
        assert_eq!(confirmed.last_seen(), None, "a stopped upload confirms nothing");
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
        let forged = FileRef::new(file_ref.id_unchecked(), file_ref.read_secret() ^ 1);
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

    /// A file whose upload never finished before a restart is persisted from its staged bytes
    /// when the database reopens, and stays hidden from confirmed reads until it is.
    #[cfg(unix)]
    #[test]
    fn staged_files_survive_a_restart_before_they_are_persisted() -> anyhow::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let stdb = TestDB::durable_with_files()?;
        let rt = stdb.runtime().expect("durable").clone();
        wait_for_snapshot(&stdb, &rt, 1);
        let objects = stdb.path().expect("durable").files().0.join("objects");
        let set_writable = |writable: bool| {
            let mode = if writable { 0o755 } else { 0o555 };
            std::fs::set_permissions(&objects, std::fs::Permissions::from_mode(mode))
        };

        // The store refuses writes, so the upload keeps failing.
        set_writable(false)?;
        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        let file_ref = create_file(&mut tx, b"survives a restart", None, Timestamp::now())?;
        let (offset, ..) = stdb.commit_tx(tx)?.expect("consumes an offset");
        std::thread::sleep(Duration::from_millis(300));
        let confirmed = stdb.durable_tx_offset().expect("durable").last_seen();
        assert!(
            confirmed < Some(offset),
            "confirmed {confirmed:?} before the file was persisted"
        );

        // Restart. The transaction is durable in the log, and its bytes with it.
        let stdb = stdb.reopen();
        set_writable(true)?;
        let stdb = stdb?;
        let rt = stdb.runtime().expect("durable").clone();
        rt.block_on(async {
            let mut confirmed = stdb.durable_tx_offset().expect("durable");
            tokio::time::timeout(Duration::from_secs(10), confirmed.wait_for(offset)).await
        })??;
        let files = stdb.files().expect("files are enabled").clone();
        let hash = content_hash(b"survives a restart");
        assert_eq!(rt.block_on(files.store().size(&hash))?, Some(18));
        let row = stdb.with_read_only(Workload::ForTests, |tx| live_file_row_for_ref(tx, file_ref))?;
        assert!(row.is_some(), "the file's record survived the restart");
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while stdb
            .with_read_only(Workload::ForTests, |tx| staged_bytes(tx, hash))?
            .is_some()
        {
            assert!(
                std::time::Instant::now() < deadline,
                "the staged row was never cleaned up"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }

    #[test]
    fn copying_shares_bytes_and_deleting_keeps_them() -> anyhow::Result<()> {
        let stdb = TestDB::durable_with_files()?;
        let now = Timestamp::now();
        let (original, copy) = stdb.with_auto_commit(Workload::ForTests, |tx| -> Result<_, DBError> {
            let original = create_file(tx, b"shared", None, now)?;
            let copy = copy_file(tx, original.id_unchecked(), now)?.expect("the original exists");
            Ok((original, copy))
        })?;
        assert_ne!(original.id_unchecked(), copy.id_unchecked());

        let deleted =
            stdb.with_auto_commit(Workload::ForTests, |tx| delete_file(&stdb, tx, original.id_unchecked()))?;
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
        assert!(!stdb.with_auto_commit(Workload::ForTests, |tx| delete_file(&stdb, tx, original.id_unchecked()))?);
        // Nor can a deleted file be copied.
        assert!(stdb
            .with_auto_commit(Workload::ForTests, |tx| copy_file(tx, original.id_unchecked(), now))?
            .is_none());

        // A reference with the wrong secret finds nothing.
        let forged = FileRef::new(copy.id_unchecked(), copy.read_secret() ^ 1);
        assert!(stdb
            .with_read_only(Workload::ForTests, |tx| live_file_row_for_ref(tx, forged))?
            .is_none());
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
            assert!(delete_file(&stdb, tx, snapshotted.id_unchecked())?);
            assert!(delete_file(&stdb, tx, deleted.id_unchecked())?);
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

    /// A staged file waits for its own persist even when its hash is known to be persisted, because a
    /// reclamation pass that began before the transaction committed may be about to delete the object.
    #[tokio::test(flavor = "multi_thread")]
    async fn staged_files_wait_for_their_own_persist() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = Handle::tokio_current();
        let store = Arc::new(HeldStore {
            inner: LocalFileStore::open(dir.path()).unwrap(),
            released: Semaphore::new(1),
        });
        let files = DatabaseFiles::new(store.clone(), runtime.clone());
        files.settle_startup();
        let hash = content_hash(b"hello");
        files.persist_staged(hash, Bytes::from_static(b"hello"));
        tokio::time::timeout(Duration::from_secs(5), async {
            while !files.is_known_persisted(&hash) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the first persist should finish");

        let inner = ManualDurability::new();
        let gated = FileGatedDurability::new(inner.clone(), files.clone(), runtime);
        let mut confirmed = gated.durable_tx_offset();
        gated.append_tx(tx(0, &[b"hello"], &[b"hello"]));
        inner.make_durable(0);
        settle().await;
        assert_eq!(confirmed.last_seen(), None, "confirmed before its own persist finished");

        store.released.add_permits(1);
        tokio::time::timeout(Duration::from_secs(5), confirmed.wait_for(0))
            .await
            .expect("the persist should finish")
            .unwrap();
    }

    /// Reclamation checks each object against the current `st_file` just before deleting it, because a
    /// transaction may have named the object after the pass gathered its roots.
    #[test]
    fn reclamation_spares_objects_named_after_its_roots_were_gathered() -> anyhow::Result<()> {
        let stdb = TestDB::durable_with_files()?;
        let rt = stdb.runtime().expect("durable").clone();
        let files = stdb.files().expect("files are enabled").clone();
        wait_for_snapshot(&stdb, &rt, 1);

        // The pass gathers its roots before the file exists.
        let stale_roots = reclamation_roots(&stdb)?;
        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        create_file(&mut tx, b"named late", None, Timestamp::now())?;
        let (offset, ..) = stdb.commit_tx(tx)?.expect("consumes an offset");
        rt.block_on(async { stdb.durable_tx_offset().expect("durable").wait_for(offset).await })?;
        let hash = content_hash(b"named late");
        assert!(!stale_roots.contains(&hash));

        // With no grace period, only the check against `st_file` saves it.
        let stats = rt.block_on(files.reclaim(stale_roots, Duration::ZERO, |hash| file_is_named(&stdb, hash)))?;
        assert_eq!(stats.deleted_objects, 0);
        assert_eq!(stats.spared_named_objects, 1);
        assert_eq!(rt.block_on(files.store().size(&hash))?, Some(10));
        Ok(())
    }

    /// An upload's object is kept from when its bytes are persisted until its guard is dropped,
    /// however much longer than the grace period that takes.
    #[tokio::test(flavor = "multi_thread")]
    async fn pending_uploads_are_not_reclaimed() {
        let dir = tempfile::tempdir().unwrap();
        let files = DatabaseFiles::new(
            Arc::new(LocalFileStore::open(dir.path()).unwrap()),
            Handle::tokio_current(),
        );
        let mut spool = files.store().spool().await.unwrap();
        spool.write(b"uploading").await.unwrap();
        let pending = files.persist(spool.finish().await.unwrap()).await.unwrap();
        let hash = content_hash(b"uploading");

        // Nothing names the object, and the grace period is over, but its row hasn't committed.
        let stats = files.reclaim(HashSet::new(), Duration::ZERO, |_| false).await.unwrap();
        assert_eq!(stats.deleted_objects, 0);
        assert_eq!(files.store().size(&hash).await.unwrap(), Some(9));

        // Once the upload is abandoned, the object goes.
        drop(pending);
        let stats = files.reclaim(HashSet::new(), Duration::ZERO, |_| false).await.unwrap();
        assert_eq!(stats.deleted_objects, 1);
        assert_eq!(files.store().size(&hash).await.unwrap(), None);
    }

    /// Reclamation deletes nothing while a snapshot is being written or compressed, since the files
    /// that snapshot references can't be listed until it's done.
    #[test]
    fn reclamation_waits_for_locked_snapshots() -> anyhow::Result<()> {
        let stdb = TestDB::durable_with_files()?;
        let rt = stdb.runtime().expect("durable").clone();
        let runtime = Handle::tokio(rt.clone());
        let files = stdb.files().expect("files are enabled").clone();
        wait_for_snapshot(&stdb, &rt, 1);
        let repo = stdb.snapshot_repo().expect("snapshots are enabled");
        let snapshot = repo.latest_snapshot()?.expect("the worker took a snapshot");
        let orphan = content_hash(b"orphan");
        rt.block_on(files.store().persist_bytes(orphan, Bytes::from_static(b"orphan")))?;

        let snapshot_dir = stdb.path().expect("durable").snapshots().snapshot_dir(snapshot);
        let lock = spacetimedb_fs_utils::lockfile::Lockfile::for_file(&snapshot_dir)?;
        assert!(rt.block_on(reclaim_files(&stdb, Duration::ZERO, &runtime)).is_err());
        assert_eq!(rt.block_on(files.store().size(&orphan))?, Some(6));

        drop(lock);
        let stats = rt.block_on(reclaim_files(&stdb, Duration::ZERO, &runtime))?;
        assert_eq!(stats.deleted_objects, 1);
        Ok(())
    }

    #[test]
    fn completed_uploads_are_found_by_sender_and_key_until_they_expire() -> anyhow::Result<()> {
        let stdb = TestDB::in_memory()?;
        let alice = Identity::from_u256(1u32.into());
        let bob = Identity::from_u256(2u32.into());
        let day = UPLOAD_KEY_RETENTION;
        let now = Timestamp::now();
        let record = |sender, key: &str, file: u128, age: Duration| StUploadRow {
            sender: IdentityViaU256(sender),
            idempotency_key: key.into(),
            file_id: FileId(file),
            completed_at: TimestampViaI64(now.checked_sub_duration(age).unwrap()),
        };
        stdb.with_auto_commit(Workload::ForTests, |tx| -> Result<(), DBError> {
            assert_eq!(
                record_upload(&stdb, tx, &record(alice, "photo", 1, Duration::ZERO), now)?,
                None
            );
            assert_eq!(
                record_upload(&stdb, tx, &record(bob, "photo", 2, Duration::ZERO), now)?,
                None
            );
            assert_eq!(record_upload(&stdb, tx, &record(alice, "old", 3, 2 * day), now)?, None);
            // A second attempt that loses the race learns which file the first one created.
            assert_eq!(
                record_upload(&stdb, tx, &record(alice, "photo", 4, Duration::ZERO), now)?,
                Some(FileId(1))
            );
            Ok(())
        })?;
        let found = |sender, key: &'static str| {
            stdb.with_read_only(Workload::ForTests, |tx| completed_upload(tx, sender, key, now))
                .map(|row| row.map(|row| row.file_id))
        };
        // Keys are scoped to their sender, and expired records are ignored even before they're deleted.
        assert_eq!(found(alice, "photo")?, Some(FileId(1)));
        assert_eq!(found(bob, "photo")?, Some(FileId(2)));
        assert_eq!(found(bob, "old")?, None);
        assert_eq!(found(alice, "old")?, None);

        // An expired key can be used again, and expiry deletes what's left.
        stdb.with_auto_commit(Workload::ForTests, |tx| -> Result<(), DBError> {
            assert_eq!(
                record_upload(&stdb, tx, &record(alice, "old", 5, Duration::ZERO), now)?,
                None
            );
            Ok(())
        })?;
        assert_eq!(found(alice, "old")?, Some(FileId(5)));
        let expired = stdb.with_auto_commit(Workload::ForTests, |tx| expire_upload_records(&stdb, tx, now, day))?;
        assert_eq!(expired, 0, "the expired record was replaced");
        Ok(())
    }

    /// A user table with one `u64` column.
    fn u64_table() -> spacetimedb_schema::schema::TableSchema {
        use spacetimedb_lib::db::raw_def::v9::RawModuleDefV9Builder;
        use spacetimedb_sats::{raw_identifier::RawIdentifier, AlgebraicType, ProductType};
        use spacetimedb_schema::schema::Schema as _;
        let mut builder = RawModuleDefV9Builder::new();
        builder.build_table_with_new_type(
            RawIdentifier::new("numbers"),
            ProductType::from([("n", AlgebraicType::U64)]),
            true,
        );
        let def: spacetimedb_schema::def::ModuleDef = builder.finish().try_into().expect("a valid table");
        let table = def.table("numbers").expect("the table exists");
        spacetimedb_schema::schema::TableSchema::from_module_def(
            &def,
            table,
            (),
            spacetimedb_primitives::TableId::SENTINEL,
        )
    }

    /// Prints how long a durable database takes to confirm transactions without files,
    /// with and without File Storage. Run with `--ignored --nocapture`.
    #[test]
    #[ignore = "a measurement, not a check"]
    #[allow(clippy::disallowed_macros, reason = "measurements print their results")]
    fn measure_gate_overhead() -> anyhow::Result<()> {
        const TXS: usize = 20_000;
        for with_files in [false, true, false, true] {
            let stdb = if with_files {
                TestDB::durable_with_files()?
            } else {
                TestDB::durable()?
            };
            let rt = stdb.runtime().expect("durable").clone();
            // Let the snapshot taken after opening finish, so it doesn't compete with the measurement.
            wait_for_snapshot(&stdb, &rt, 1);
            let schema = u64_table();
            let table_id = stdb.with_auto_commit(Workload::ForTests, |tx| stdb.create_table(tx, schema))?;
            let mut durable = stdb.durable_tx_offset().expect("durable");
            let start = std::time::Instant::now();
            let mut last = 0;
            for i in 0..TXS {
                let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
                stdb.insert(
                    &mut tx,
                    table_id,
                    &spacetimedb_sats::bsatn::to_vec(&spacetimedb_sats::product![i as u64])?,
                )?;
                last = stdb.commit_tx(tx)?.expect("consumes an offset").0;
            }
            let committed = start.elapsed();
            rt.block_on(durable.wait_for(last))?;
            let confirmed = start.elapsed();
            println!(
                "files={with_files}: {TXS} txs committed in {committed:?}, all confirmed after {confirmed:?} ({:?}/tx)",
                confirmed / TXS as u32
            );
        }
        Ok(())
    }

    /// Prints how long a snapshot takes with many files, since the manifest's file list is built
    /// by scanning `st_file` while the snapshot holds the datastore lock. Run with `--ignored --nocapture`.
    #[test]
    #[ignore = "a measurement, not a check"]
    #[allow(clippy::disallowed_macros, reason = "measurements print their results")]
    fn measure_snapshot_with_many_files() -> anyhow::Result<()> {
        for files in [0, 100_000, 1_000_000] {
            let stdb = TestDB::durable()?;
            let rt = stdb.runtime().expect("durable").clone();
            wait_for_snapshot(&stdb, &rt, 1);
            const BATCH: usize = 50_000;
            for batch in 0..files / BATCH {
                stdb.with_auto_commit(Workload::ForTests, |tx| -> Result<(), DBError> {
                    for i in 0..BATCH {
                        let n = (batch * BATCH + i) as u64;
                        let mut row = file_row(&n.to_le_bytes());
                        // Half the files share contents, as copies would.
                        row.content_hash = content_hash(&(n / 2).to_le_bytes());
                        insert_file_row(tx, &row)?;
                    }
                    Ok(())
                })?;
            }
            // The same work the snapshot does under its lock to build the manifest's file list.
            let start = std::time::Instant::now();
            let distinct = stdb.with_read_only(Workload::ForTests, |tx| -> Result<usize, DBError> {
                let mut hashes = BTreeMap::new();
                for row in tx.iter(ST_FILE_ID)? {
                    let row = StFileRow::try_from(row)?;
                    hashes.insert(row.content_hash, row.size);
                }
                Ok(hashes.len())
            })?;
            let scan = start.elapsed();
            let repo = stdb.snapshot_repo().expect("snapshots are enabled");
            let start = std::time::Instant::now();
            stdb.take_snapshot(&*repo)?.expect("a snapshot is taken");
            let snapshot = start.elapsed();
            assert_eq!(repo.retained_file_hashes()?.len(), distinct);
            println!(
                "{files} files ({distinct} distinct hashes): listing them took {scan:?}; \
                 the whole snapshot, including st_file's pages, took {snapshot:?}"
            );
        }
        Ok(())
    }
}
