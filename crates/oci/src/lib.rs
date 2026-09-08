//! Validate immutable OCI objects before accepting a container deployment.
//!
//! Registry references and index annotations are discovery inputs. Only verified
//! object bytes and an exact selected platform establish the published image.

pub mod layers;

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spacetimedb_lib::container::{ImagePlatform, OciDigest, MAX_ARGV_ENTRIES, MAX_ENV_KEYS, MAX_EXEC_STRING_BYTES};
use std::collections::BTreeMap;

pub const OCI_MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";
pub const OCI_INDEX: &str = "application/vnd.oci.image.index.v1+json";
pub const OCI_CONFIG: &str = "application/vnd.oci.image.config.v1+json";
pub const DOCKER_MANIFEST: &str = "application/vnd.docker.distribution.manifest.v2+json";
pub const DOCKER_INDEX: &str = "application/vnd.docker.distribution.manifest.list.v2+json";
pub const DOCKER_CONFIG: &str = "application/vnd.docker.container.image.v1+json";
pub const MAX_MANIFEST_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_CONFIG_BYTES: usize = 1024 * 1024;
pub const MAX_LAYERS: usize = 256;
pub const MAX_INDEX_ENTRIES: usize = 256;
pub const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Descriptor {
    pub media_type: String,
    pub digest: OciDigest,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<Platform>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_type: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Platform {
    pub os: String,
    pub architecture: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default, rename = "os.version", skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    #[serde(default, rename = "os.features", skip_serializing_if = "Vec::is_empty")]
    pub os_features: Vec<String>,
}

impl Platform {
    fn matches(&self, requested: &ImagePlatform) -> bool {
        self.os == requested.os
            && self.architecture == requested.architecture
            && self.os_version.as_deref().is_none_or(str::is_empty)
            && self.os_features.is_empty()
            && matches!(
                (self.architecture.as_str(), self.variant.as_deref()),
                (_, None | Some("")) | ("arm64", Some("v8"))
            )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: u32,
    pub media_type: String,
    pub config: Descriptor,
    pub layers: Vec<Descriptor>,
    #[serde(default)]
    artifact_type: Option<String>,
    #[serde(default)]
    subject: Option<Descriptor>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageIndex {
    pub schema_version: u32,
    pub media_type: String,
    pub manifests: Vec<Descriptor>,
}

pub fn sha256(bytes: &[u8]) -> OciDigest {
    OciDigest::sha256(Sha256::digest(bytes).into())
}

pub fn verify_object(descriptor: &Descriptor, bytes: &[u8]) -> Result<()> {
    ensure!(
        descriptor.size == bytes.len() as u64,
        "OCI object length differs from its descriptor"
    );
    ensure!(
        sha256(bytes) == descriptor.digest,
        "OCI object SHA-256 differs from its descriptor"
    );
    Ok(())
}

fn validate_descriptor(descriptor: &Descriptor, max_bytes: u64) -> Result<()> {
    ensure!(
        descriptor.size > 0 && descriptor.size <= max_bytes,
        "OCI descriptor size exceeds admission bounds"
    );
    ensure!(
        descriptor.urls.is_empty(),
        "external OCI descriptor URLs are not supported"
    );
    ensure!(descriptor.data.is_none(), "inline OCI descriptor data is not supported");
    ensure!(
        descriptor.artifact_type.is_none(),
        "OCI artifacts are not executable images"
    );
    Ok(())
}

pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest> {
    ensure!(bytes.len() <= MAX_MANIFEST_BYTES, "OCI manifest is too large");
    let manifest: Manifest = serde_json::from_slice(bytes).context("invalid OCI image manifest")?;
    ensure!(manifest.schema_version == 2, "unsupported OCI manifest schema version");
    ensure!(
        matches!(manifest.media_type.as_str(), OCI_MANIFEST | DOCKER_MANIFEST),
        "unsupported image manifest media type"
    );
    ensure!(
        manifest.artifact_type.is_none() && manifest.subject.is_none(),
        "OCI artifact manifests are not executable images"
    );
    ensure!(manifest.layers.len() <= MAX_LAYERS, "too many OCI image layers");
    validate_descriptor(&manifest.config, MAX_CONFIG_BYTES as u64)?;
    ensure!(
        matches!(manifest.config.media_type.as_str(), OCI_CONFIG | DOCKER_CONFIG),
        "unsupported image config media type"
    );
    let mut total = manifest.config.size;
    for layer in &manifest.layers {
        validate_descriptor(layer, MAX_IMAGE_BYTES)?;
        ensure!(
            matches!(
                layer.media_type.as_str(),
                "application/vnd.oci.image.layer.v1.tar"
                    | "application/vnd.oci.image.layer.v1.tar+gzip"
                    | "application/vnd.oci.image.layer.v1.tar+zstd"
                    | "application/vnd.docker.image.rootfs.diff.tar"
                    | "application/vnd.docker.image.rootfs.diff.tar.gzip"
            ),
            "unsupported or foreign image layer media type"
        );
        total = total.checked_add(layer.size).context("OCI image size overflow")?;
    }
    ensure!(total <= MAX_IMAGE_BYTES, "OCI image exceeds compressed object quota");
    Ok(manifest)
}

/// Select exactly one executable image for the requested platform. BuildKit may
/// include attestation descriptors for unknown/unknown; they are never executed.
pub fn select_platform(bytes: &[u8], requested: &ImagePlatform) -> Result<Descriptor> {
    validate_platform(requested)?;
    ensure!(bytes.len() <= MAX_MANIFEST_BYTES, "OCI image index is too large");
    let index: ImageIndex = serde_json::from_slice(bytes).context("invalid OCI image index")?;
    ensure!(
        index.schema_version == 2 && matches!(index.media_type.as_str(), OCI_INDEX | DOCKER_INDEX),
        "unsupported image index format"
    );
    ensure!(
        index.manifests.len() <= MAX_INDEX_ENTRIES,
        "too many image index entries"
    );
    let mut selected = None;
    for descriptor in index.manifests {
        if !descriptor.platform.as_ref().is_some_and(|p| p.matches(requested)) {
            continue;
        }
        validate_descriptor(&descriptor, MAX_MANIFEST_BYTES as u64)?;
        ensure!(
            matches!(descriptor.media_type.as_str(), OCI_MANIFEST | DOCKER_MANIFEST),
            "selected platform is not an image manifest"
        );
        ensure!(
            selected.replace(descriptor).is_none(),
            "OCI index has ambiguous images for the selected platform"
        );
    }
    selected.context("OCI image does not contain the selected Linux platform")
}

fn validate_platform(platform: &ImagePlatform) -> Result<()> {
    ensure!(
        platform.os == "linux" && matches!(platform.architecture.as_str(), "amd64" | "arm64"),
        "unsupported container platform"
    );
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageConfig {
    pub architecture: String,
    pub os: String,
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub config: ContainerConfig,
    pub rootfs: RootFs,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ContainerConfig {
    #[serde(default)]
    pub entrypoint: Option<Vec<String>>,
    #[serde(default)]
    pub cmd: Option<Vec<String>>,
    #[serde(default)]
    pub user: String,
    #[serde(default, rename = "WorkingDir")]
    pub working_directory: String,
    #[serde(default)]
    pub env: Option<Vec<String>>,
    #[serde(default)]
    pub volumes: Option<BTreeMap<String, serde_json::Value>>,
}

impl std::fmt::Debug for ContainerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContainerConfig")
            .field("entrypoint", &self.entrypoint)
            .field("cmd", &self.cmd)
            .field("user", &self.user)
            .field("working_directory", &self.working_directory)
            .field(
                "env_keys",
                &self.env.as_ref().map(|env| {
                    env.iter()
                        .map(|v| v.split('=').next().unwrap_or(""))
                        .collect::<Vec<_>>()
                }),
            )
            .field("volumes", &self.volumes.as_ref().map(|v| v.keys().collect::<Vec<_>>()))
            .finish()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RootFs {
    #[serde(rename = "type")]
    pub kind: String,
    pub diff_ids: Vec<OciDigest>,
}

pub fn parse_config(bytes: &[u8], manifest: &Manifest, platform: &ImagePlatform) -> Result<ImageConfig> {
    validate_platform(platform)?;
    ensure!(bytes.len() <= MAX_CONFIG_BYTES, "OCI image config is too large");
    verify_object(&manifest.config, bytes)?;
    let image: ImageConfig = serde_json::from_slice(bytes).context("invalid OCI image config")?;
    ensure!(
        Platform {
            os: image.os.clone(),
            architecture: image.architecture.clone(),
            variant: image.variant.clone(),
            os_version: None,
            os_features: vec![]
        }
        .matches(platform),
        "image config platform does not match selected platform"
    );
    ensure!(
        image.rootfs.kind == "layers" && image.rootfs.diff_ids.len() == manifest.layers.len(),
        "image rootfs does not match layer descriptors"
    );
    ensure!(
        image.config.volumes.as_ref().is_none_or(BTreeMap::is_empty),
        "image-declared volumes are unsupported in Stage 1"
    );
    image.config.environment()?;
    for argv in [&image.config.entrypoint, &image.config.cmd].into_iter().flatten() {
        ensure!(argv.len() <= MAX_ARGV_ENTRIES, "too many image command arguments");
        for arg in argv {
            validate_string(arg)?;
        }
    }
    validate_string(&image.config.user)?;
    validate_string(&image.config.working_directory)?;
    ensure!(
        image.config.working_directory.is_empty() || image.config.working_directory.starts_with('/'),
        "image working directory must be absolute"
    );
    Ok(image)
}

fn validate_string(value: &str) -> Result<()> {
    ensure!(
        value.len() < MAX_EXEC_STRING_BYTES && !value.contains('\0'),
        "invalid container startup string"
    );
    Ok(())
}

impl ContainerConfig {
    /// Keep image defaults separately from the normalized spec. Environment
    /// values remain in the immutable image, never in hot deployment metadata.
    pub fn environment(&self) -> Result<BTreeMap<String, String>> {
        let env = self.env.as_deref().unwrap_or_default();
        ensure!(env.len() <= MAX_ENV_KEYS, "too many image environment variables");
        let mut result = BTreeMap::new();
        for value in env {
            validate_string(value)?;
            let (key, value) = value
                .split_once('=')
                .context("image environment entry must contain '='")?;
            ensure!(
                valid_env_key(key) && !key.starts_with("SPACETIMEDB_"),
                "invalid or reserved image environment key"
            );
            ensure!(
                result.insert(key.to_owned(), value.to_owned()).is_none(),
                "duplicate image environment key"
            );
        }
        Ok(result)
    }

    pub fn argv(&self, override_command: Option<&[String]>) -> Result<Vec<String>> {
        let argv = match override_command {
            Some(argv) => argv.to_vec(),
            None => self
                .entrypoint
                .iter()
                .flatten()
                .chain(self.cmd.iter().flatten())
                .cloned()
                .collect(),
        };
        ensure!(
            !argv.is_empty() && !argv[0].is_empty() && argv.len() <= MAX_ARGV_ENTRIES,
            "image needs a nonempty main command"
        );
        for arg in &argv {
            validate_string(arg)?;
        }
        Ok(argv)
    }
}

pub fn valid_env_key(key: &str) -> bool {
    let mut bytes = key.bytes();
    key.len() <= 256
        && bytes.next().is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Descriptor closure used to pin all required objects before desired-state
/// commit. Reject a digest repeated with conflicting lengths or media types.
pub fn object_closure(manifest_descriptor: Descriptor, manifest: &Manifest) -> Result<Vec<Descriptor>> {
    validate_descriptor(&manifest_descriptor, MAX_MANIFEST_BYTES as u64)?;
    let mut seen = BTreeMap::new();
    let mut objects = Vec::with_capacity(manifest.layers.len() + 2);
    for descriptor in std::iter::once(manifest_descriptor)
        .chain(std::iter::once(manifest.config.clone()))
        .chain(manifest.layers.iter().cloned())
    {
        match seen.insert(descriptor.digest, (descriptor.size, descriptor.media_type.clone())) {
            Some(previous) if previous != (descriptor.size, descriptor.media_type.clone()) => {
                bail!("OCI digest has conflicting descriptors")
            }
            Some(_) => {}
            None => objects.push(descriptor),
        }
    }
    Ok(objects)
}

#[cfg(test)]
mod tests;
