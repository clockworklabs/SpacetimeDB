//! A [`FileStore`] in an S3-compatible bucket.
//!
//! Objects live at `{prefix}objects/{hash}`. Spools are local files, so an upload is hashed while it
//! arrives and stored only once it is complete and verified. Small files go up in one request and
//! larger ones in parts.
//!
//! S3 can't change an existing object's modification time, which reclamation uses to spare objects
//! persisted again while it runs. So persisting an object that already exists writes an empty marker
//! at `{prefix}refreshed/{hash}`, and an object's `stored_at` is the later of the object's and its
//! marker's.
//!
//! S3 has no conditional delete that would make [`FileStore::delete_if_older_than`] atomic, so it
//! checks and then deletes. Within one host, `DatabaseFiles` excludes persists of the same hash while
//! it does; hosts that share a bucket would need a lease or a second grace period.

use crate::{ByteStream, FileStore, Spool, Spooled, StoredObject};
use async_trait::async_trait;
use bytes::Bytes;
use futures::{StreamExt, TryStreamExt};
use object_store::path::Path;
use object_store::{GetOptions, GetRange, ObjectStore, PutPayload, WriteMultipart};
use spacetimedb_lib::Hash;
use std::collections::HashMap;
use std::io;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::io::AsyncReadExt;

/// Files at least this large are uploaded in parts of this size.
const PART_SIZE: usize = 8 * 1024 * 1024;

/// Where an [`S3FileStore`] keeps objects.
#[derive(Clone, Debug)]
pub struct S3Config {
    /// The endpoint, such as `https://s3.us-east-1.amazonaws.com` or a local S3-compatible server.
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    /// Prepended to every key, such as the database family's name followed by `/`.
    pub prefix: String,
    /// Credentials, or `None` for a server that accepts unsigned requests.
    pub credentials: Option<(String, String)>,
}

pub struct S3FileStore {
    store: Arc<dyn ObjectStore>,
    prefix: String,
    spools: PathBuf,
}

fn to_io(err: object_store::Error) -> io::Error {
    match err {
        object_store::Error::NotFound { .. } => io::Error::new(io::ErrorKind::NotFound, err),
        err => io::Error::other(err),
    }
}

impl S3FileStore {
    /// Opens the store, keeping spools in `spools`, and deletes spools left by a previous process.
    pub fn open(config: S3Config, spools: impl Into<PathBuf>) -> io::Result<Self> {
        let mut builder = object_store::aws::AmazonS3Builder::new()
            .with_endpoint(&config.endpoint)
            .with_region(&config.region)
            .with_bucket_name(&config.bucket)
            .with_allow_http(config.endpoint.starts_with("http://"));
        builder = match &config.credentials {
            Some((key, secret)) => builder.with_access_key_id(key).with_secret_access_key(secret),
            None => builder.with_skip_signature(true),
        };
        let store = builder.build().map_err(io::Error::other)?;
        let spools = spools.into();
        std::fs::create_dir_all(&spools)?;
        for entry in std::fs::read_dir(&spools)? {
            let entry = entry?;
            if let Err(e) = std::fs::remove_file(entry.path()) {
                log::warn!("failed to remove stale spool {}: {e}", entry.path().display());
            }
        }
        Ok(Self {
            store: Arc::new(store),
            prefix: config.prefix,
            spools,
        })
    }

    fn object_path(&self, hash: &Hash) -> Path {
        Path::from(format!("{}objects/{}", self.prefix, hash.to_hex()))
    }

    fn marker_path(&self, hash: &Hash) -> Path {
        Path::from(format!("{}refreshed/{}", self.prefix, hash.to_hex()))
    }

    /// When the object was last persisted, counting refresh markers, or `None` if it doesn't exist.
    async fn stored_at(&self, hash: &Hash) -> io::Result<Option<SystemTime>> {
        let object = match self.store.head(&self.object_path(hash)).await {
            Ok(meta) => meta.last_modified,
            Err(object_store::Error::NotFound { .. }) => return Ok(None),
            Err(e) => return Err(to_io(e)),
        };
        let marker = match self.store.head(&self.marker_path(hash)).await {
            Ok(meta) => Some(meta.last_modified),
            Err(object_store::Error::NotFound { .. }) => None,
            Err(e) => return Err(to_io(e)),
        };
        Ok(Some(object.max(marker.unwrap_or(object)).into()))
    }

    async fn upload(&self, path: &Path, spooled: &Spooled) -> io::Result<()> {
        let mut file = tokio::fs::File::open(spooled.path()).await?;
        if spooled.size() < PART_SIZE as u64 {
            let mut bytes = Vec::with_capacity(spooled.size() as usize);
            file.read_to_end(&mut bytes).await?;
            self.store.put(path, PutPayload::from(bytes)).await.map_err(to_io)?;
            return Ok(());
        }
        let upload = self.store.put_multipart(path).await.map_err(to_io)?;
        let mut writer = WriteMultipart::new_with_chunk_size(upload, PART_SIZE);
        let mut buf = vec![0; PART_SIZE];
        loop {
            let n = file.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            writer.wait_for_capacity(4).await.map_err(to_io)?;
            writer.write(&buf[..n]);
        }
        writer.finish().await.map_err(to_io)?;
        Ok(())
    }
}

#[async_trait]
impl FileStore for S3FileStore {
    async fn spool(&self) -> io::Result<Spool> {
        Spool::create_in(&self.spools).await
    }

    async fn persist(&self, spooled: Spooled) -> io::Result<()> {
        let hash = spooled.hash();
        if self.stored_at(&hash).await?.is_some() {
            // Refresh the object's age, so a reclamation pass that already chose it spares it.
            self.store
                .put(&self.marker_path(&hash), PutPayload::default())
                .await
                .map_err(to_io)?;
            return Ok(());
        }
        // Each upload's key is its hash, so a concurrent upload of the same contents writes the
        // same bytes, and S3's last-writer-wins can't corrupt the object.
        self.upload(&self.object_path(&hash), &spooled).await
    }

    async fn size(&self, hash: &Hash) -> io::Result<Option<u64>> {
        match self.store.head(&self.object_path(hash)).await {
            Ok(meta) => Ok(Some(meta.size)),
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(to_io(e)),
        }
    }

    async fn read(&self, hash: &Hash, range: Range<u64>) -> io::Result<ByteStream> {
        if range.start == range.end {
            // S3 rejects empty ranges; check that the object exists, then return nothing.
            self.size(hash)
                .await?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such object"))?;
            return Ok(Box::pin(futures::stream::empty()));
        }
        let options = GetOptions {
            range: Some(GetRange::Bounded(range)),
            ..Default::default()
        };
        let result = self
            .store
            .get_opts(&self.object_path(hash), options)
            .await
            .map_err(to_io)?;
        Ok(Box::pin(result.into_stream().map_err(to_io)))
    }

    async fn list(&self) -> io::Result<Vec<StoredObject>> {
        let markers: HashMap<String, SystemTime> = self
            .store
            .list(Some(&Path::from(format!("{}refreshed", self.prefix))))
            .map_ok(|meta| {
                let name = meta.location.filename().unwrap_or_default().to_owned();
                (name, SystemTime::from(meta.last_modified))
            })
            .try_collect()
            .await
            .map_err(to_io)?;
        let mut objects = Vec::new();
        let mut listing = self.store.list(Some(&Path::from(format!("{}objects", self.prefix))));
        while let Some(meta) = listing.next().await {
            let meta = meta.map_err(to_io)?;
            let Some(name) = meta.location.filename() else { continue };
            let Ok(hash) = Hash::from_hex(name) else { continue };
            let modified = SystemTime::from(meta.last_modified);
            let refreshed = markers.get(name).copied().unwrap_or(modified);
            objects.push(StoredObject {
                hash,
                size: meta.size,
                stored_at: modified.max(refreshed),
            });
        }
        Ok(objects)
    }

    async fn delete(&self, hash: &Hash) -> io::Result<()> {
        for path in [self.object_path(hash), self.marker_path(hash)] {
            match self.store.delete(&path).await {
                Ok(()) | Err(object_store::Error::NotFound { .. }) => {}
                Err(e) => return Err(to_io(e)),
            }
        }
        Ok(())
    }

    async fn delete_if_older_than(&self, hash: &Hash, cutoff: SystemTime) -> io::Result<bool> {
        match self.stored_at(hash).await? {
            Some(stored_at) if stored_at < cutoff => {
                self.delete(hash).await?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn persist_bytes(&self, hash: Hash, bytes: Bytes) -> io::Result<()> {
        let mut spool = self.spool().await?;
        spool.write(&bytes).await?;
        let spooled = spool.finish().await?;
        if spooled.hash() != hash {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("expected contents with hash {hash}, got {}", spooled.hash()),
            ));
        }
        self.persist(spooled).await
    }
}
