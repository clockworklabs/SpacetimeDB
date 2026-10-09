//! Tests of [`S3FileStore`] against a real S3-compatible server.
//!
//! They run only when `SPACETIMEDB_TEST_S3_ENDPOINT` names a server with a bucket called
//! `SPACETIMEDB_TEST_S3_BUCKET` (default `stdb-files`) that accepts unsigned requests,
//! or signed ones if `SPACETIMEDB_TEST_S3_ACCESS_KEY` and `SPACETIMEDB_TEST_S3_SECRET_KEY` are set.
//! For example, with SeaweedFS:
//!
//! ```sh
//! docker run -d -p 127.0.0.1:18333:8333 chrislusf/seaweedfs server -s3 -dir=/data
//! curl -X PUT http://127.0.0.1:18333/stdb-files
//! SPACETIMEDB_TEST_S3_ENDPOINT=http://127.0.0.1:18333 cargo test -p spacetimedb-file-store --features s3
//! ```
#![cfg(feature = "s3")]
#![allow(clippy::disallowed_macros, reason = "tests say why they skip")]

use bytes::Bytes;
use futures::TryStreamExt;
use spacetimedb_file_store::{reclaim_unreferenced, FileStore, S3Config, S3FileStore};
use spacetimedb_lib::file::content_hash;
use spacetimedb_lib::Hash;
use std::collections::HashSet;
use std::io;
use std::ops::Range;
use std::time::{Duration, SystemTime};

/// A store under a fresh prefix, or `None` if no test server is configured.
fn store() -> Option<(S3FileStore, tempfile::TempDir)> {
    let Ok(endpoint) = std::env::var("SPACETIMEDB_TEST_S3_ENDPOINT") else {
        eprintln!("skipping: SPACETIMEDB_TEST_S3_ENDPOINT is not set");
        return None;
    };
    let bucket = std::env::var("SPACETIMEDB_TEST_S3_BUCKET").unwrap_or_else(|_| "stdb-files".into());
    let credentials = std::env::var("SPACETIMEDB_TEST_S3_ACCESS_KEY")
        .ok()
        .zip(std::env::var("SPACETIMEDB_TEST_S3_SECRET_KEY").ok());
    let prefix = format!(
        "test-{}-{}/",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let spools = tempfile::tempdir().unwrap();
    let config = S3Config {
        endpoint,
        region: "us-east-1".into(),
        bucket,
        prefix,
        credentials,
    };
    Some((S3FileStore::open(config, spools.path()).unwrap(), spools))
}

async fn read_all(store: &dyn FileStore, hash: &Hash, range: Range<u64>) -> Vec<u8> {
    let chunks: Vec<Bytes> = store.read(hash, range).await.unwrap().try_collect().await.unwrap();
    chunks.concat()
}

#[tokio::test]
async fn small_objects_round_trip_by_range() {
    let Some((store, _spools)) = store() else { return };
    let bytes = Bytes::from_static(b"hello, files");
    let hash = content_hash(&bytes);
    store.persist_bytes(hash, bytes).await.unwrap();
    assert_eq!(store.size(&hash).await.unwrap(), Some(12));
    assert_eq!(read_all(&store, &hash, 0..12).await, b"hello, files");
    assert_eq!(read_all(&store, &hash, 7..12).await, b"files");
    assert_eq!(read_all(&store, &hash, 3..3).await, b"");
}

#[tokio::test]
async fn large_objects_upload_in_parts() {
    let Some((store, spools)) = store() else { return };
    // Three 8 MiB parts and a remainder.
    let bytes: Vec<u8> = (0..(26 * 1024 * 1024 + 123)).map(|i: u32| (i % 251) as u8).collect();
    let mut spool = store.spool().await.unwrap();
    for chunk in bytes.chunks(1 << 20) {
        spool.write(chunk).await.unwrap();
    }
    let spooled = spool.finish().await.unwrap();
    let hash = spooled.hash();
    assert_eq!(hash, content_hash(&bytes));
    store.persist(spooled).await.unwrap();

    assert_eq!(store.size(&hash).await.unwrap(), Some(bytes.len() as u64));
    // A range across a part boundary.
    let start = 8 * 1024 * 1024 - 10;
    let range = start as u64..start as u64 + 20;
    assert_eq!(read_all(&store, &hash, range).await, &bytes[start..start + 20]);
    assert_eq!(read_all(&store, &hash, 0..bytes.len() as u64).await, bytes);
    // The spool was cleaned up.
    assert_eq!(std::fs::read_dir(spools.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn missing_objects_are_not_found() {
    let Some((store, _spools)) = store() else { return };
    let hash = content_hash(b"never stored");
    assert_eq!(store.size(&hash).await.unwrap(), None);
    let err = store.read(&hash, 0..1).await.err().expect("no such object");
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
    store.delete(&hash).await.unwrap();
}

#[tokio::test]
async fn persisting_again_saves_an_object_from_reclamation() {
    let Some((store, _spools)) = store() else { return };
    let bytes = Bytes::from_static(b"again");
    let hash = content_hash(&bytes);
    store.persist_bytes(hash, bytes.clone()).await.unwrap();
    assert_eq!(store.list().await.unwrap().len(), 1);

    // S3 timestamps have one-second resolution.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let cutoff = SystemTime::now();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    // Persisting again stores nothing new, but marks the object as refreshed.
    store.persist_bytes(hash, bytes).await.unwrap();
    assert_eq!(store.list().await.unwrap().len(), 1, "still one object");
    assert!(!store.delete_if_older_than(&hash, cutoff).await.unwrap());
    assert!(store.size(&hash).await.unwrap().is_some());

    let later = SystemTime::now() + Duration::from_secs(5);
    assert!(store.delete_if_older_than(&hash, later).await.unwrap());
    assert_eq!(store.size(&hash).await.unwrap(), None);
}

#[tokio::test]
async fn reclamation_deletes_only_unreferenced_objects() {
    let Some((store, _spools)) = store() else { return };
    let mut hashes = Vec::new();
    for contents in [&b"kept"[..], b"garbage"] {
        let hash = content_hash(contents);
        store
            .persist_bytes(hash, Bytes::copy_from_slice(contents))
            .await
            .unwrap();
        hashes.push(hash);
    }
    let [kept, garbage] = hashes[..] else { unreachable!() };
    let listed: HashSet<Hash> = store.list().await.unwrap().iter().map(|o| o.hash).collect();
    assert_eq!(listed, HashSet::from([kept, garbage]));

    // Pretend the grace period has passed by reclaiming as of a moment in the future.
    let later = SystemTime::now() + Duration::from_secs(5);
    let stats = reclaim_unreferenced(&store, &HashSet::from([kept]), Duration::ZERO, later)
        .await
        .unwrap();
    assert_eq!(stats.deleted_objects, 1);
    assert_eq!(store.size(&garbage).await.unwrap(), None);
    assert!(store.size(&kept).await.unwrap().is_some());
}
