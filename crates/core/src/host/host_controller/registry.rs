//! Canonical host cells, pinned by queued and active operations.
//!
//! Lock order: the registry mutex is never held across an await. A cell guard
//! may briefly acquire the registry mutex to publish its resident-host state.
//! It releases the cell guard before releasing its operation pin. The last pin
//! removes an empty, non-closing entry without waiting for unrelated Arc owners.

use super::{Host, HostCell};
use parking_lot::Mutex;
use spacetimedb_data_structures::map::IntMap;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Weak};
use tokio::sync::{watch, OwnedRwLockReadGuard, OwnedRwLockWriteGuard};

pub(super) type Hosts = Arc<Mutex<IntMap<u64, Entry>>>;

pub(super) struct Entry {
    cell: HostCell,
    pins: usize,
    resident: bool,
    registration: Option<Arc<()>>,
    closing: Option<Arc<Closing>>,
}

/// Completion means the storage writer has actually closed, not that the
/// caller's wait expired. Provider-owned snapshot/archival services are separate.
pub(super) struct Closing {
    result: watch::Sender<Option<Result<(), Arc<str>>>>,
}

impl Closing {
    pub async fn wait(&self) -> anyhow::Result<()> {
        let mut receiver = self.result.subscribe();
        let result = receiver.wait_for(Option::is_some).await?.clone().unwrap();
        result.map_err(|message| anyhow::anyhow!(message.to_string()))
    }
}

/// A pin counts an operation, including time queued for the cell lock. Idle
/// ModuleHost references are not pins. Raw cells never authorize an operation.
pub(super) struct Pin {
    hosts: Weak<Mutex<IntMap<u64, Entry>>>,
    replica: u64,
    cell: HostCell,
}

impl Pin {
    pub fn acquire(hosts: &Hosts, replica: u64) -> Self {
        let mut entries = hosts.lock();
        let entry = entries.entry(replica).or_insert_with(|| Entry {
            cell: HostCell::default(),
            pins: 0,
            resident: false,
            registration: None,
            closing: None,
        });
        entry.pins += 1;
        Self {
            hosts: Arc::downgrade(hosts),
            replica,
            cell: entry.cell.clone(),
        }
    }

    fn closing(&self) -> Option<Arc<Closing>> {
        let hosts = self.hosts.upgrade()?;
        let entries = hosts.lock();
        let entry = entries.get(&self.replica)?;
        debug_assert!(Arc::ptr_eq(&entry.cell, &self.cell));
        entry.closing.clone()
    }

    pub async fn read(hosts: &Hosts, replica: u64) -> anyhow::Result<ReadGuard> {
        loop {
            let pin = Self::acquire(hosts, replica);
            let guard = pin.cell.clone().read_owned().await;
            if let Some(closing) = pin.closing() {
                drop(guard);
                drop(pin);
                closing.wait().await?;
                continue;
            }
            return Ok(ReadGuard {
                guard: Some(guard),
                pin: Some(pin),
            });
        }
    }

    pub async fn write(hosts: &Hosts, replica: u64) -> anyhow::Result<WriteGuard> {
        loop {
            let pin = Self::acquire(hosts, replica);
            let guard = pin.cell.clone().write_owned().await;
            if let Some(closing) = pin.closing() {
                drop(guard);
                drop(pin);
                closing.wait().await?;
                continue;
            }
            return Ok(WriteGuard {
                guard: Some(guard),
                pin: Some(pin),
            });
        }
    }

    fn publish(&self, host: Option<&Host>) {
        let Some(hosts) = self.hosts.upgrade() else { return };
        let mut entries = hosts.lock();
        let Some(entry) = entries.get_mut(&self.replica) else {
            return;
        };
        debug_assert!(Arc::ptr_eq(&entry.cell, &self.cell));
        entry.resident = host.is_some();
        entry.registration = host.map(|host| host.registration.token.clone());
    }

    fn registration(&self) -> Registration {
        Registration {
            hosts: self.hosts.clone(),
            replica: self.replica,
            cell: Arc::downgrade(&self.cell),
            token: Arc::new(()),
        }
    }
}

impl Drop for Pin {
    fn drop(&mut self) {
        let Some(hosts) = self.hosts.upgrade() else { return };
        let removed = {
            let mut entries = hosts.lock();
            let Some(entry) = entries.get_mut(&self.replica) else {
                return;
            };
            assert!(Arc::ptr_eq(&entry.cell, &self.cell), "replaced a pinned host cell");
            entry.pins -= 1;
            if entry.pins == 0 && !entry.resident && entry.closing.is_none() {
                entries.remove(&self.replica)
            } else {
                None
            }
        };
        drop(removed);
    }
}

pub(super) struct ReadGuard {
    guard: Option<OwnedRwLockReadGuard<Option<Host>>>,
    pin: Option<Pin>,
}

impl Deref for ReadGuard {
    type Target = Option<Host>;
    fn deref(&self) -> &Self::Target {
        self.guard.as_ref().unwrap()
    }
}

impl Drop for ReadGuard {
    fn drop(&mut self) {
        drop(self.guard.take());
        drop(self.pin.take());
    }
}

pub(super) struct WriteGuard {
    guard: Option<OwnedRwLockWriteGuard<Option<Host>>>,
    pin: Option<Pin>,
}

impl WriteGuard {
    pub fn registration(&self) -> Registration {
        self.pin.as_ref().unwrap().registration()
    }

    pub fn install(&mut self, host: Host) {
        **self = Some(host);
        self.pin.as_ref().unwrap().publish(self.as_ref());
    }

    pub fn quarantine(&self) {
        let pin = self.pin.as_ref().unwrap();
        let Some(hosts) = pin.hosts.upgrade() else { return };
        let mut entries = hosts.lock();
        let entry = entries.get_mut(&pin.replica).expect("pinned cell exists");
        if let Some(closing) = &entry.closing {
            closing
                .result
                .send_replace(Some(Err("storage writer close is unconfirmed".into())));
        }
        entry.closing = Some(Arc::new(Closing {
            result: watch::Sender::new(Some(Err(
                "storage writer close is unconfirmed; replica is quarantined".into()
            ))),
        }));
    }
}

impl Deref for WriteGuard {
    type Target = Option<Host>;
    fn deref(&self) -> &Self::Target {
        self.guard.as_ref().unwrap()
    }
}
impl DerefMut for WriteGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.as_mut().unwrap()
    }
}
impl Drop for WriteGuard {
    fn drop(&mut self) {
        self.pin.as_ref().unwrap().publish(self.as_ref());
        drop(self.guard.take());
        drop(self.pin.take());
    }
}

/// The token changes with each installed executable, even when its cell and
/// database stay the same. A late panic from an old executable is not authority
/// to close its replacement.
#[derive(Clone)]
pub(super) struct Registration {
    hosts: Weak<Mutex<IntMap<u64, Entry>>>,
    replica: u64,
    cell: Weak<tokio::sync::RwLock<Option<Host>>>,
    token: Arc<()>,
}

impl Registration {
    pub fn activate(&self) {
        let (Some(hosts), Some(cell)) = (self.hosts.upgrade(), self.cell.upgrade()) else {
            return;
        };
        let mut entries = hosts.lock();
        if let Some(entry) = entries.get_mut(&self.replica)
            && Arc::ptr_eq(&entry.cell, &cell)
        {
            entry.registration = Some(self.token.clone());
        }
    }

    pub fn close_if_current(&self) -> Option<CloseRequest> {
        let (Some(hosts), Some(cell)) = (self.hosts.upgrade(), self.cell.upgrade()) else {
            return None;
        };
        let mut entries = hosts.lock();
        let entry = entries.get_mut(&self.replica)?;
        if !Arc::ptr_eq(&entry.cell, &cell)
            || !entry
                .registration
                .as_ref()
                .is_some_and(|token| Arc::ptr_eq(token, &self.token))
        {
            return None;
        }
        Some(request_close(&hosts, self.replica, entry))
    }
}

pub(super) struct CloseOwner {
    pin: Pin,
    completion: Arc<Closing>,
}

pub(super) struct CloseRequest {
    pub completion: Arc<Closing>,
    pub owner: Option<CloseOwner>,
}

pub(super) fn close(hosts: &Hosts, replica: u64) -> Option<CloseRequest> {
    let mut entries = hosts.lock();
    let entry = entries.get_mut(&replica)?;
    Some(request_close(hosts, replica, entry))
}

fn request_close(hosts: &Hosts, replica: u64, entry: &mut Entry) -> CloseRequest {
    if let Some(completion) = &entry.closing {
        return CloseRequest {
            completion: completion.clone(),
            owner: None,
        };
    }
    let completion = Arc::new(Closing {
        result: watch::Sender::new(None),
    });
    entry.closing = Some(completion.clone());
    entry.pins += 1;
    CloseRequest {
        completion: completion.clone(),
        owner: Some(CloseOwner {
            pin: Pin {
                hosts: Arc::downgrade(hosts),
                replica,
                cell: entry.cell.clone(),
            },
            completion,
        }),
    }
}

impl CloseOwner {
    pub async fn run(self) {
        let mut guard = self.pin.cell.clone().write_owned().await;
        // A prior cold owner can report an unconfirmed writer before this
        // queued close obtains its guard. An empty cell is not proof of closure.
        if self.completion.result.borrow().is_some() {
            return;
        }
        let result = match guard.take() {
            Some(host) => super::lifecycle::close_host(host).await,
            None => Ok(()),
        };
        let writer_closed = !matches!(result, Err(super::lifecycle::CloseFailure::WriterUnconfirmed));
        // Publish under the cell lock, then release it before final registry
        // unpin/removal. Closing stays set during both steps, so reopen waits.
        self.pin.publish(None);
        drop(guard);
        if let Some(hosts) = self.pin.hosts.upgrade() {
            let mut entries = hosts.lock();
            if let Some(entry) = entries.get_mut(&self.pin.replica) {
                debug_assert!(Arc::ptr_eq(&entry.cell, &self.pin.cell));
                if writer_closed {
                    entry.closing = None;
                }
            }
        }
        self.completion
            .result
            .send_replace(Some(result.map_err(|error| Arc::from(error.to_string()))));
        // self.pin drops last. An unrelated idle Arc cannot delay completion.
    }
}
