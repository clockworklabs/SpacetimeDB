use async_trait::async_trait;
use spacetimedb_lib::{hash_bytes, Hash};
use std::io;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;

use super::ExternalStorage;

/// A simple [`ExternalStorage`] that stores programs in the filesystem.
#[derive(Clone, Debug)]
pub struct DiskStorage {
    base: PathBuf,
}

impl DiskStorage {
    pub async fn new(base: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&base).await?;
        Ok(Self { base })
    }

    fn object_path(&self, h: &Hash) -> PathBuf {
        let hex = h.to_hex();
        let (pre, suf) = hex.split_at(2);
        let mut path = self.base.clone();
        path.extend([pre, suf]);
        path
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn get(&self, key: &Hash) -> io::Result<Option<Box<[u8]>>> {
        let path = self.object_path(key);
        match fs::read(path).await {
            Ok(bytes) => {
                let actual_hash = hash_bytes(&bytes);
                if actual_hash == *key {
                    Ok(Some(bytes.into()))
                } else {
                    log::warn!("hash mismatch: {actual_hash} stored at {key}");
                    if let Err(e) = self.prune(key).await {
                        log::warn!("prune error: {e}");
                    }
                    Ok(None)
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    #[tracing::instrument(level = "trace", skip(self, value))]
    pub async fn put(&self, value: &[u8]) -> io::Result<Hash> {
        let h = hash_bytes(value);
        let path = self.object_path(&h);
        fs::create_dir_all(path.parent().expect("object path must have a parent")).await?;

        // to ensure it doesn't conflict with a concurrent call to put() - suffix with nanosecond timestamp
        let ts = std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos();
        let tmp = path.with_extension(format!("tmp{ts}"));
        {
            let mut f = fs::File::options().write(true).create_new(true).open(&tmp).await?;
            f.write_all(value).await?;
            f.sync_data().await?;
        }

        fs::rename(tmp, path).await?;

        Ok(h)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn prune(&self, key: &Hash) -> anyhow::Result<()> {
        Ok(fs::remove_file(self.object_path(key)).await?)
    }
}

#[async_trait]
impl ExternalStorage for DiskStorage {
    async fn lookup(&self, program_hash: Hash) -> anyhow::Result<Option<Box<[u8]>>> {
        self.get(&program_hash).await.map_err(Into::into)
    }
}
