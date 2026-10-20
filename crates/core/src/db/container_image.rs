//! Container images stored in the database's `st_container_image` table, as `st_module` stores
//! the module's program.
//!
//! An image is the gzipped output of `docker save`, under its local image ID. Table data lives in
//! memory and goes into the commitlog and snapshots, so images are limited to
//! [`MAX_IMAGE_BYTES`] and a database keeps only two: the one its container uses, and the newest.
//! Only the container API writes this table; modules and SQL DML cannot.

use std::io::Read;

use super::relational_db::{MutTx, RelationalDB};
use crate::error::DBError;
use spacetimedb_datastore::error::DatastoreError;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_datastore::system_tables::{
    read_bytes_from_col, StContainerImageFields, StContainerImageRow, ST_CONTAINER_IMAGE_ID,
};
use spacetimedb_lib::container::is_local_image_id;
pub use spacetimedb_lib::container::MAX_IMAGE_BYTES;
use spacetimedb_lib::Timestamp;
use spacetimedb_sats::{bsatn, AlgebraicValue};

/// Whether the database stores the image `image_id`.
pub fn contains(state: &impl StateView, image_id: &str) -> Result<bool, DatastoreError> {
    Ok(state
        .iter_by_col_eq(
            ST_CONTAINER_IMAGE_ID,
            StContainerImageFields::ImageId,
            &AlgebraicValue::String(image_id.into()),
        )?
        .next()
        .is_some())
}

/// The stored image `image_id`: the gzipped output of `docker save`.
pub fn get(state: &impl StateView, image_id: &str) -> Result<Option<Box<[u8]>>, DatastoreError> {
    state
        .iter_by_col_eq(
            ST_CONTAINER_IMAGE_ID,
            StContainerImageFields::ImageId,
            &AlgebraicValue::String(image_id.into()),
        )?
        .next()
        .map(|row| read_bytes_from_col(row, StContainerImageFields::Image))
        .transpose()
}

/// Store `image` as the image `image_id` unless the database already has it. Of the other
/// images, keep only `in_use`, the image of the database's container, if stored, or else the
/// newest. Returns whether it was stored. Check it with [`verify`] first.
pub fn store(
    db: &RelationalDB,
    tx: &mut MutTx,
    image_id: &str,
    image: Box<[u8]>,
    created_at: Timestamp,
    in_use: Option<&str>,
) -> Result<bool, DBError> {
    if contains(tx, image_id)? {
        return Ok(false);
    }
    let mut previous = tx
        .iter(ST_CONTAINER_IMAGE_ID)?
        .map(|row| {
            let id = row.read_col::<Box<str>>(StContainerImageFields::ImageId)?;
            let created_at = row.read_col::<i64>(StContainerImageFields::CreatedAt)?;
            Ok(((Some(&*id) == in_use, created_at), row.pointer()))
        })
        .collect::<Result<Vec<_>, DBError>>()?;
    // The image in use, or else the newest, sorts last.
    previous.sort_unstable_by_key(|(key, _)| *key);
    previous.pop();
    db.delete(tx, ST_CONTAINER_IMAGE_ID, previous.into_iter().map(|(_, ptr)| ptr));
    let size = image.len() as u64;
    let row = StContainerImageRow {
        image_id: image_id.to_owned(),
        image,
        size,
        created_at: created_at.into(),
    };
    // Not `insert_via_serialize_bsatn`, whose thread-local buffer would keep the image's size.
    let row = bsatn::to_vec(&row).map_err(anyhow::Error::from)?;
    db.insert(tx, ST_CONTAINER_IMAGE_ID, &row)?;
    Ok(true)
}

/// Check that `image` is the gzipped output of `docker save` for the image `image_id`.
///
/// The ID is the digest of the image's config with Docker's classic image store, and of its
/// manifest or index with the containerd image store. The archive must contain the blob with
/// that digest and name it as an image. It must contain only that image, untagged, as
/// `docker save <image ID>` writes it, so that loading it tags no other image.
pub fn verify(image: &[u8], image_id: &str) -> Result<(), String> {
    if !is_local_image_id(image_id) {
        return Err(format!(
            "`{image_id}` is not an image ID, like `sha256:<64 hex digits>`"
        ));
    }
    let hex = &image_id["sha256:".len()..];
    // `docker save` names blobs `blobs/sha256/<hex>`, or, before Docker 25, configs `<hex>.json`.
    let blob_paths = [format!("blobs/sha256/{hex}"), format!("{hex}.json")];
    let not_archive = |e: std::io::Error| format!("the image must be the gzipped output of `docker save`: {e}");
    // Bounds the work a highly compressed archive can cause.
    const MAX_UNPACKED: u64 = 16 * MAX_IMAGE_BYTES;
    let unpacked = flate2::read::MultiGzDecoder::new(image).take(MAX_UNPACKED);
    let mut archive = tar::Archive::new(unpacked);
    let (mut manifest, mut index, mut blob) = (None, None, None);
    for entry in archive.entries().map_err(not_archive)? {
        let mut entry = entry.map_err(not_archive)?;
        let path = entry.path().map_err(not_archive)?;
        let path = path.to_string_lossy();
        let path = path.trim_start_matches("./").to_owned();
        match &*path {
            "manifest.json" => manifest = Some(read_json(&mut entry).map_err(not_archive)?),
            "index.json" => index = Some(read_json(&mut entry).map_err(not_archive)?),
            _ if blob_paths.contains(&path) && sha256_hex(&mut entry).map_err(not_archive)? == hex => blob = Some(path),
            _ => {}
        }
    }
    if archive.into_inner().limit() == 0 {
        return Err(format!("the image unpacks to more than {} GiB", MAX_UNPACKED >> 30));
    }
    let Some(blob) = blob else {
        return Err(format!("the archive does not contain the image {image_id}"));
    };
    let is_config = |manifest: &serde_json::Value| manifest["Config"].as_str() == Some(&blob);
    let is_manifest = |descriptor: &serde_json::Value| descriptor["digest"].as_str() == Some(image_id);
    let named = manifest
        .as_ref()
        .and_then(|manifest| manifest.as_array())
        .is_some_and(|manifests| manifests.iter().any(is_config))
        || index
            .as_ref()
            .and_then(|index| index["manifests"].as_array())
            .is_some_and(|descriptors| descriptors.iter().any(is_manifest));
    if !named {
        return Err(format!("the archive does not name {image_id} as an image"));
    }
    let tagged = |manifest: &serde_json::Value| !manifest["RepoTags"].as_array().is_none_or(Vec::is_empty);
    let named_by_ref = |descriptor: &serde_json::Value| {
        let annotations = &descriptor["annotations"];
        !annotations["io.containerd.image.name"].is_null()
            || !annotations["org.opencontainers.image.ref.name"].is_null()
    };
    let other = manifest
        .as_ref()
        .and_then(|manifest| manifest.as_array())
        .is_some_and(|manifests| manifests.len() > 1 || manifests.iter().any(tagged))
        || index
            .as_ref()
            .and_then(|index| index["manifests"].as_array())
            .is_some_and(|descriptors| descriptors.len() > 1 || descriptors.iter().any(named_by_ref));
    if other {
        return Err(format!(
            "the archive must contain only the image {image_id}, untagged, as `docker save {image_id}` writes it"
        ));
    }
    Ok(())
}

fn read_json(entry: &mut impl Read) -> std::io::Result<serde_json::Value> {
    // Manifests are small; this bounds what a malformed archive can make us buffer.
    let mut json = Vec::new();
    entry.take(16 << 20).read_to_end(&mut json)?;
    serde_json::from_slice(&json).map_err(std::io::Error::other)
}

fn sha256_hex(entry: &mut impl Read) -> std::io::Result<String> {
    let mut hasher = openssl::sha::Sha256::new();
    let mut buf = vec![0; 64 << 10];
    loop {
        match entry.read(&mut buf)? {
            0 => break,
            n => hasher.update(&buf[..n]),
        }
    }
    Ok(hasher.finish().iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::relational_db::tests_utils::TestDB;
    use flate2::{write::GzEncoder, Compression};
    use spacetimedb_datastore::execution_context::Workload;

    fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
        for (path, contents) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, path, *contents).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn digest(contents: &[u8]) -> String {
        sha256_hex(&mut &contents[..]).unwrap()
    }

    #[test]
    fn verifies_saved_images() {
        let config = br#"{"architecture":"arm64"}"#;
        let manifest_blob = br#"{"config":{}}"#;
        let (config_hex, manifest_hex) = (digest(config), digest(manifest_blob));
        let config_path = format!("blobs/sha256/{config_hex}");
        let manifest_path = format!("blobs/sha256/{manifest_hex}");
        let manifest = format!(r#"[{{"Config":"{config_path}","RepoTags":null,"Layers":[]}}]"#);
        let index = format!(r#"{{"manifests":[{{"digest":"sha256:{manifest_hex}"}}]}}"#);
        let saved = archive(&[
            (&config_path, config),
            (&manifest_path, manifest_blob),
            ("index.json", index.as_bytes()),
            ("manifest.json", manifest.as_bytes()),
        ]);
        // The classic image store's ID, and the containerd image store's.
        verify(&saved, &format!("sha256:{config_hex}")).unwrap();
        verify(&saved, &format!("sha256:{manifest_hex}")).unwrap();

        let other = format!("sha256:{}", "0".repeat(64));
        assert!(verify(&saved, &other).unwrap_err().contains("does not contain"));
        assert!(verify(&saved, "latest").unwrap_err().contains("not an image ID"));
        assert!(verify(b"not gzip", &other).unwrap_err().contains("gzipped output"));
        // A blob with the right digest that no manifest names is not an image.
        let unnamed = archive(&[(&config_path, config), ("manifest.json", b"[]")]);
        assert!(verify(&unnamed, &format!("sha256:{config_hex}"))
            .unwrap_err()
            .contains("does not name"));
        // A blob whose contents do not match its name.
        let tampered = archive(&[(&config_path, b"{}"), ("manifest.json", manifest.as_bytes())]);
        assert!(verify(&tampered, &format!("sha256:{config_hex}")).is_err());
        // Loading a tagged image would retag it.
        let manifest = format!(r#"[{{"Config":"{config_path}","RepoTags":["nginx:latest"],"Layers":[]}}]"#);
        let tagged = archive(&[(&config_path, config), ("manifest.json", manifest.as_bytes())]);
        assert!(verify(&tagged, &format!("sha256:{config_hex}"))
            .unwrap_err()
            .contains("untagged"));
    }

    #[test]
    fn keeps_the_image_in_use_or_the_newest() -> anyhow::Result<()> {
        let db = TestDB::in_memory()?;
        let id = |n: u8| format!("sha256:{}", format!("{n:x}").repeat(64));
        let at = Timestamp::from_micros_since_unix_epoch;
        let store = |n: u8, micros, in_use: Option<u8>| {
            db.with_auto_commit(Workload::ForTests, |tx| {
                let in_use = in_use.map(id);
                store(&db, tx, &id(n), vec![n; 3].into(), at(micros), in_use.as_deref())
            })
        };
        let stored = |n: u8| db.with_read_only(Workload::ForTests, |tx| contains(tx, &id(n)));
        assert!(store(1, 1, None)?);
        assert!(store(2, 2, None)?);
        assert!(!store(1, 3, None)?, "an image is stored once");
        // Without an image in use, the newest stays.
        assert!(store(3, 4, None)?);
        assert!(!stored(1)? && stored(2)? && stored(3)?);
        // The image in use stays, though it is older.
        assert!(store(4, 5, Some(2))?);
        assert!(stored(2)? && !stored(3)? && stored(4)?);
        let image = db.with_read_only(Workload::ForTests, |tx| get(tx, &id(4)))?;
        assert_eq!(image.as_deref(), Some(&[4; 3][..]));
        Ok(())
    }
}
