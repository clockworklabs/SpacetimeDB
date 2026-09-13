//! Bounded OCI import. Layers are inspected as streams, never unpacked into the project.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spacetimedb_lib::container::{ImagePlatform, OciDigest};
use spacetimedb_oci::{
    self as oci,
    layers::{verify_layer_with_check, LayerLimits},
    Descriptor, ImageConfig,
};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

pub const MAX_EXPANDED_IMAGE_BYTES: u64 = 128 * 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 1024;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Manifest,
    Config,
    Layer,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalArtifact {
    pub kind: ArtifactKind,
    pub descriptor: Descriptor,
    /// Relative to the owned OCI layout directory.
    pub path: PathBuf,
}
pub(crate) struct VerifiedImage {
    pub manifest: Descriptor,
    pub config: ImageConfig,
    pub objects: Vec<LocalArtifact>,
}

pub(crate) fn check(cancel: &CancellationToken, deadline: Instant) -> std::io::Result<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "container preparation cancelled or timed out",
        ))
    } else {
        Ok(())
    }
}
fn blob_path(digest: OciDigest) -> PathBuf {
    PathBuf::from("blobs/sha256").join(digest.to_string().strip_prefix("sha256:").expect("SHA-256 digest"))
}
fn bounded_file(path: &Path, limit: u64) -> Result<File> {
    ensure!(
        fs::symlink_metadata(path)?.is_file(),
        "OCI object must be a regular file"
    );
    let file = File::open(path)?;
    ensure!(file.metadata()?.len() <= limit, "OCI object exceeds its size bound");
    Ok(file)
}
fn read_small(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = vec![];
    bounded_file(path, limit as u64)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "OCI metadata exceeds its size bound");
    Ok(bytes)
}

pub(crate) fn extract_archive(
    archive: &Path,
    output: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<()> {
    fs::create_dir_all(output.join("blobs/sha256"))?;
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    let archive = bounded_file(archive, oci::MAX_IMAGE_BYTES + 16 * 1024 * 1024)?;
    for (index, entry) in tar::Archive::new(archive).entries()?.enumerate() {
        check(cancel, deadline)?;
        ensure!(index < MAX_ARCHIVE_ENTRIES, "too many OCI archive entries");
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let name = path.to_str().context("OCI archive path is not UTF-8")?;
        if entry.header().entry_type().is_dir() {
            ensure!(
                matches!(name.trim_end_matches('/'), "." | "blobs" | "blobs/sha256"),
                "unexpected OCI archive directory"
            );
            continue;
        }
        ensure!(
            entry.header().entry_type().is_file(),
            "OCI archive links and special files are unsupported"
        );
        let blob = name.strip_prefix("blobs/sha256/");
        ensure!(
            matches!(name, "index.json" | "oci-layout")
                || blob.is_some_and(
                    |v| v.len() == 64 && v.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
                ),
            "unexpected OCI archive path"
        );
        ensure!(seen.insert(path.clone()), "duplicate OCI archive path");
        let size = entry.size();
        total = total.checked_add(size).context("OCI archive size overflow")?;
        ensure!(
            total <= oci::MAX_IMAGE_BYTES + 16 * 1024 * 1024,
            "OCI archive exceeds image size bound"
        );
        if blob.is_none() {
            ensure!(size <= oci::MAX_MANIFEST_BYTES as u64, "OCI archive metadata too large");
        }
        let mut file = File::options().write(true).create_new(true).open(output.join(&path))?;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            check(cancel, deadline)?;
            let n = entry.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            file.write_all(&buffer[..n])?;
        }
    }
    Ok(())
}

pub(crate) fn verify_layout(
    input: &Path,
    output: &Path,
    platform: &ImagePlatform,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<VerifiedImage> {
    check(cancel, deadline)?;
    let layout: serde_json::Value = serde_json::from_slice(&read_small(&input.join("oci-layout"), 1024)?)?;
    ensure!(
        layout.get("imageLayoutVersion").and_then(|v| v.as_str()) == Some("1.0.0"),
        "unsupported OCI layout version"
    );
    let index = read_small(&input.join("index.json"), oci::MAX_MANIFEST_BYTES)?;
    // Layout metadata is not a hashed image object and may omit mediaType.
    // Defaults apply only here, never to immutable registry index bytes.
    let mut index: serde_json::Value = serde_json::from_slice(&index)?;
    index
        .as_object_mut()
        .context("OCI layout index must be an object")?
        .entry("mediaType")
        .or_insert_with(|| oci::OCI_INDEX.into());
    let index = serde_json::to_vec(&index)?;
    let parsed: oci::ImageIndex = serde_json::from_slice(&index)?;
    ensure!(
        parsed.media_type == oci::OCI_INDEX,
        "unsupported OCI layout index media type"
    );
    ensure!(
        parsed.schema_version == 2 && parsed.manifests.len() <= oci::MAX_INDEX_ENTRIES,
        "invalid OCI layout index"
    );
    // A local layout index commonly names one manifest without platform metadata.
    // The immutable image config below must still match the explicit platform.
    let descriptor = if parsed.manifests.len() == 1 && parsed.manifests[0].platform.is_none() {
        parsed.manifests.into_iter().next().unwrap()
    } else {
        oci::select_platform(&index, platform)?
    };
    // A layout can name a multi-platform index; select exactly one executable manifest.
    let source = read_small(&input.join(blob_path(descriptor.digest)), oci::MAX_MANIFEST_BYTES)?;
    oci::verify_object(&descriptor, &source)?;
    let (descriptor, bytes) = if matches!(descriptor.media_type.as_str(), oci::OCI_INDEX | oci::DOCKER_INDEX) {
        let selected = oci::select_platform(&source, platform)?;
        let bytes = read_small(&input.join(blob_path(selected.digest)), oci::MAX_MANIFEST_BYTES)?;
        oci::verify_object(&selected, &bytes)?;
        (selected, bytes)
    } else {
        (descriptor, source)
    };
    ensure!(
        matches!(descriptor.media_type.as_str(), oci::OCI_MANIFEST | oci::DOCKER_MANIFEST),
        "OCI layout does not select an executable image"
    );
    let manifest = oci::parse_manifest(&bytes)?;
    let config_bytes = read_small(&input.join(blob_path(manifest.config.digest)), oci::MAX_CONFIG_BYTES)?;
    let config = oci::parse_config(&config_bytes, &manifest, platform)?;
    let mut compressed = 0u64;
    let mut expanded = 0u64;
    let mut entries = 0u64;
    for (layer, diff_id) in manifest.layers.iter().zip(&config.rootfs.diff_ids) {
        check(cancel, deadline)?;
        compressed = compressed.checked_add(layer.size).context("image size overflow")?;
        ensure!(compressed <= oci::MAX_IMAGE_BYTES, "compressed image exceeds bound");
        let file = bounded_file(&input.join(blob_path(layer.digest)), layer.size)?;
        let size = verify_layer_with_check(file, layer, *diff_id, LayerLimits::default(), || {
            check(cancel, deadline)
        })?;
        expanded = expanded
            .checked_add(size.uncompressed_tar_bytes)
            .context("expanded image size overflow")?;
        entries = entries
            .checked_add(size.entries)
            .context("image entry count overflow")?;
        ensure!(
            expanded <= MAX_EXPANDED_IMAGE_BYTES && entries <= 1_000_000,
            "expanded image exceeds bound"
        );
    }
    fs::create_dir_all(output.join("blobs/sha256"))?;
    let mut objects = vec![];
    for object in oci::object_closure(descriptor.clone(), &manifest)? {
        check(cancel, deadline)?;
        let path = blob_path(object.digest);
        let mut source = bounded_file(&input.join(&path), object.size)?;
        source.rewind()?;
        let mut target = File::options().write(true).create_new(true).open(output.join(&path))?;
        let mut hash = Sha256::new();
        let mut total = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            check(cancel, deadline)?;
            let n = source.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            total = total.checked_add(n as u64).context("object size overflow")?;
            ensure!(total <= object.size, "OCI object changed while copying");
            hash.update(&buffer[..n]);
            target.write_all(&buffer[..n])?;
        }
        ensure!(
            total == object.size && OciDigest::sha256(hash.finalize().into()) == object.digest,
            "OCI object changed while copying"
        );
        objects.push(LocalArtifact {
            kind: if object.digest == descriptor.digest {
                ArtifactKind::Manifest
            } else if object.digest == manifest.config.digest {
                ArtifactKind::Config
            } else {
                ArtifactKind::Layer
            },
            descriptor: object,
            path,
        });
    }
    fs::write(output.join("oci-layout"), br#"{"imageLayoutVersion":"1.0.0"}"#)?;
    fs::write(
        output.join("index.json"),
        serde_json::to_vec(
            &serde_json::json!({"schemaVersion":2,"mediaType":oci::OCI_INDEX,"manifests":[descriptor]}),
        )?,
    )?;
    Ok(VerifiedImage {
        manifest: descriptor,
        config,
        objects,
    })
}
