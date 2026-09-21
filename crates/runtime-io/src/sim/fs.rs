use alloc::{boxed::Box, collections::BTreeMap, sync::Arc, vec::Vec};
use core::mem;

use spin::Mutex;

use crate::{sim::SECTOR_SIZE64, SECTOR_SIZE};

type SectorId = usize;
type FileId = usize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("file already exists")]
    FileAlreadyExists,
    #[error("file not found")]
    FileNotFound,
    #[error("no space left on device")]
    NoSpace,
    #[error("invalid argument")]
    InvalidArgument,
}

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug)]
struct Sector {
    volatile: Box<[u8; SECTOR_SIZE]>,
    durable: Box<[u8; SECTOR_SIZE]>,
}

#[derive(Debug)]
struct FileState {
    // Logical sector index -> physical sector.
    sectors: Vec<SectorId>,

    volatile_len: u64,
    durable_len: u64,

    // Incremented whenever a datasync snapshot is created.
    dirty_generation: u64,
    // Logical sector -> generation it was last dirtied.
    dirty_sectors: BTreeMap<usize, u64>,
    // Generation in which the length was last dirtied.
    dirty_len: Option<u64>,
}

/// A virtual filesystem that keeps track of files, allocated space and
/// name->file mappings.
#[derive(Clone, Debug)]
pub struct Filesystem {
    inner: Arc<Mutex<FsInner>>,
}

#[derive(Debug)]
struct FsInner {
    sectors: Vec<Sector>,
    free: Vec<SectorId>,

    files: Vec<FileState>,
    paths: BTreeMap<Box<str>, FileId>,
}

/// A virtual file in the [Filesystem].
#[derive(Clone, Debug)]
pub struct File {
    fs: Filesystem,
    id: FileId,
}

/// Individual effects produced by [File::prepare_datasync].
///
/// `fsync` / `fdatasync` is modelled as a series of sector effects and
/// potentially the file length. This allows to inject failures, in particular
/// partial failure of a sync operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Datasync {
    Sector { sector: usize, generation: u64 },
    Length { generation: u64 },
}

impl Filesystem {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity.is_multiple_of(SECTOR_SIZE));

        let sector_count = capacity / SECTOR_SIZE;
        let sectors = (0..sector_count)
            .map(|_| Sector {
                volatile: Box::new([0; SECTOR_SIZE]),
                durable: Box::new([0; SECTOR_SIZE]),
            })
            .collect();

        Self {
            inner: Arc::new(Mutex::new(FsInner {
                sectors,
                free: (0..sector_count).rev().collect(),
                files: Vec::new(),
                paths: BTreeMap::new(),
            })),
        }
    }

    pub fn create(&self, path: Box<str>) -> Result<File> {
        let mut fs = self.inner.lock();

        if fs.paths.contains_key(&path) {
            return Err(Error::FileAlreadyExists);
        }

        let id = fs.files.len();

        fs.files.push(FileState {
            sectors: Vec::new(),
            volatile_len: 0,
            durable_len: 0,
            dirty_generation: 0,
            dirty_sectors: BTreeMap::new(),
            dirty_len: None,
        });

        fs.paths.insert(path, id);

        Ok(File { fs: self.clone(), id })
    }

    pub fn open(&self, path: &str) -> Result<File> {
        let fs = self.inner.lock();

        let id = *fs.paths.get(path).ok_or(Error::FileNotFound)?;

        Ok(File { fs: self.clone(), id })
    }

    /// Simulate power loss.
    ///
    /// All unsynced data and metadata are discarded.
    pub fn power_loss(&self) {
        let mut fs = self.inner.lock();

        for file_id in 0..fs.files.len() {
            let durable_len = fs.files[file_id].durable_len;
            let durable_sectors = sector_count(durable_len);

            // Allocations beyond the durable file length were not durably
            // reachable, so they become free again.
            while fs.files[file_id].sectors.len() > durable_sectors {
                let sector_id = fs.files[file_id].sectors.pop().unwrap();
                fs.free.push(sector_id);
            }

            let sector_ids = fs.files[file_id].sectors.clone();

            for sector_id in sector_ids {
                let sector = &mut fs.sectors[sector_id];
                sector.volatile.copy_from_slice(&*sector.durable);
            }

            let file = &mut fs.files[file_id];
            file.volatile_len = file.durable_len;
            file.dirty_generation = 0;
            file.dirty_sectors.clear();
            file.dirty_len = None;
        }
    }
}

impl File {
    pub(super) fn len(&self) -> u64 {
        self.fs.inner.lock().files[self.id].volatile_len
    }

    /// Preallocate enough physical sectors to cover `new_len`.
    ///
    /// Growing fails with [Error::NoSpace] if the fixed filesystem pool cannot
    /// satisfy the allocation.
    ///
    /// Shrinking is deliberately not handled by this operation.
    ///
    /// The result of this operation is not durable until [Self::apply_datasync]
    /// is called.
    pub(super) fn reserve(&self, new_len: u64) -> Result<()> {
        let mut fs = self.fs.inner.lock();
        grow(&mut fs, self.id, new_len)
    }

    /// Read one complete sector.
    ///
    /// Returns 0 at or beyond EOF. Reading one sector is atomic.
    pub(super) fn read_sector(&self, dst: &mut [u8; SECTOR_SIZE], sector: usize) -> Result<usize> {
        let fs = self.fs.inner.lock();
        let file = &fs.files[self.id];

        let offset = sector as u64 * SECTOR_SIZE64;
        if offset >= file.volatile_len {
            return Ok(0);
        }

        let sector_id = file.sectors[sector];
        dst.copy_from_slice(&fs.sectors[sector_id].volatile[..]);

        Ok(SECTOR_SIZE)
    }

    /// Write one complete sector.
    ///
    /// Like `pwrite`, this may extend the file. Extending can fail with
    /// [Error::NoSpace]. Writing one sector is atomic.
    pub(super) fn write_sector(&self, src: &[u8; SECTOR_SIZE], sector: usize) -> Result<usize> {
        let mut fs = self.fs.inner.lock();

        let end = (sector + 1).checked_mul(SECTOR_SIZE).ok_or(Error::InvalidArgument)? as u64;
        grow(&mut fs, self.id, end)?;

        let sector_id = fs.files[self.id].sectors[sector];

        fs.sectors[sector_id].volatile.copy_from_slice(src);
        let generation = fs.files[self.id].dirty_generation;
        fs.files[self.id].dirty_sectors.insert(sector, generation);

        Ok(SECTOR_SIZE)
    }

    /// Produce a series of [Datasync] effects to model `fdatasync`.
    ///
    /// The iterator captures sectors that are marked dirty at the time this
    /// method is called, and ignores sectors dirtied later (while traversing
    /// the iterator). Similarly for the file length.
    ///
    /// This is not an accurate model of how `fdatasync` works, but allows
    /// interleaving of effects and injection of faults to produce
    /// partially-durable states.
    ///
    /// [Datasync] effects are executed via [Self::apply_datasync].
    pub(super) fn prepare_datasync(&self) -> IterDatasync {
        let mut fs = self.fs.inner.lock();
        let file = &mut fs.files[self.id];

        let generation = file.dirty_generation;
        file.dirty_generation += 1;

        IterDatasync {
            file: self.clone(),
            generation,
            next_sector: 0,
            length_pending: true,
        }
    }

    /// Execute one [Datasync] effect produced by [Self::prepare_datasync].
    ///
    /// A sector is removed from the dirty set only after it succeeds.
    pub(super) fn apply_datasync(&self, effect: Datasync) -> Result<()> {
        let mut fs = self.fs.inner.lock();

        match effect {
            Datasync::Sector { sector, generation } => {
                // The sector may cease to exist if the file was concurrently
                // truncated (not currently supported). Treat such an obsolete
                // sync effect as already satisfied.
                let Some(&sector_id) = fs.files[self.id].sectors.get(sector) else {
                    fs.files[self.id].dirty_sectors.remove(&sector);
                    return Ok(());
                };

                // Persist what is visible when this effect executes.
                let volatile = fs.sectors[sector_id].volatile.clone();
                fs.sectors[sector_id].durable.copy_from_slice(&*volatile);

                // Clear dirty flag only if the sector hasn't been modified
                // since.
                let file = &mut fs.files[self.id];
                if file
                    .dirty_sectors
                    .get(&sector)
                    .is_some_and(|&dirty_generation| dirty_generation <= generation)
                {
                    file.dirty_sectors.remove(&sector);
                }
            }

            Datasync::Length { generation } => {
                let volatile_len = fs.files[self.id].volatile_len;
                let file = &mut fs.files[self.id];
                // Persist the length visible when this effect executes.
                file.durable_len = volatile_len;
                // Clear dirty flag only if no resize happened since.
                if file
                    .dirty_len
                    .is_some_and(|dirty_generation| dirty_generation <= generation)
                {
                    file.dirty_len = None;
                }
            }
        }

        Ok(())
    }
}

pub(super) struct IterDatasync {
    file: File,
    generation: u64,
    next_sector: usize,
    length_pending: bool,
}

impl Iterator for IterDatasync {
    type Item = Datasync;

    fn next(&mut self) -> Option<Self::Item> {
        let fs = self.file.fs.inner.lock();
        let file = &fs.files[self.file.id];

        if let Some((&sector, _)) = file
            .dirty_sectors
            .range(self.next_sector..)
            .find(|(_, generation)| **generation <= self.generation)
        {
            self.next_sector = sector + 1;
            return Some(Datasync::Sector {
                sector,
                generation: self.generation,
            });
        }

        if mem::take(&mut self.length_pending) && file.dirty_len.is_some_and(|generation| generation <= self.generation)
        {
            return Some(Datasync::Length {
                generation: self.generation,
            });
        }

        None
    }
}

fn sector_count(len: u64) -> usize {
    len.div_ceil(SECTOR_SIZE64) as usize
}

fn grow(fs: &mut FsInner, file_id: FileId, new_len: u64) -> Result<()> {
    let old_len = fs.files[file_id].volatile_len;

    if new_len < old_len {
        return Err(Error::InvalidArgument);
    }

    if new_len == old_len {
        return Ok(());
    }

    let old_sector_count = sector_count(old_len);
    let new_sector_count = sector_count(new_len);
    let needed = new_sector_count - old_sector_count;

    // Make growth atomic with respect to ENOSPC: either all required sectors
    // are allocated, or nothing changes.
    if fs.free.len() < needed {
        return Err(Error::NoSpace);
    }

    for _ in 0..needed {
        let sector_id = fs.free.pop().unwrap();

        // Newly allocated filesystem space reads as zero. Clearing both copies
        // also prevents data from a previous owner from becoming observable.
        fs.sectors[sector_id].volatile.fill(0);
        fs.sectors[sector_id].durable.fill(0);

        fs.files[file_id].sectors.push(sector_id);
    }

    let file = &mut fs.files[file_id];
    file.volatile_len = new_len;
    file.dirty_len = Some(file.dirty_generation);

    Ok(())
}
