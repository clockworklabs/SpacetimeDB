//! Shared, versioned container deployment data.
//!
//! A decoded declaration is untrusted. Publication must normalize and validate
//! the complete effective deployment before hashing or admitting it. Runtime
//! capabilities and database authorization are additional admission checks.

use crate::{bsatn, hash_bytes, Hash, SpacetimeType};
use std::{collections::BTreeSet, fmt, str::FromStr};

/// Version of the normalized deployment encoding, independent of module ABI.
pub const CONTAINER_SPEC_VERSION: u32 = 1;
pub const MAX_ARGV_ENTRIES: usize = 256;
pub const MAX_ENV_KEYS: usize = 256;
pub const MAX_PORTS: usize = 16;
pub const MAX_EXEC_STRING_BYTES: usize = 32 * 1024;
/// Includes NUL terminators, 64-bit pointer arrays, and reserved startup space.
pub const MAX_EXEC_BYTES: usize = 128 * 1024;
pub const EXEC_RESERVED_BYTES: usize = 4096;
pub const DEFAULT_STOP_GRACE_MS: u32 = 30_000;
pub const MAX_STOP_GRACE_MS: u32 = 120_000;

/// The digest of an OCI object. This is never a SpacetimeDB Keccak-256 program key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, SpacetimeType)]
#[sats(crate = crate)]
pub enum OciDigest {
    Sha256(Hash),
}

impl OciDigest {
    pub const fn sha256(bytes: [u8; 32]) -> Self {
        Self::Sha256(Hash::from_byte_array(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        match self {
            Self::Sha256(hash) => &hash.data,
        }
    }
}

impl fmt::Display for OciDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sha256(bytes) => write!(f, "sha256:{}", bytes.to_hex()),
        }
    }
}

impl FromStr for OciDigest {
    type Err = ContainerValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let hex = value
            .strip_prefix("sha256:")
            .ok_or_else(|| invalid("image_manifest", "only sha256 OCI digests are supported"))?;
        if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return Err(invalid(
                "image_manifest",
                "expected 64 lowercase hexadecimal digest characters",
            ));
        }
        let mut bytes = [0; 32];
        hex::decode_to_slice(hex, &mut bytes).map_err(|_| invalid("image_manifest", "invalid SHA-256 digest"))?;
        Ok(Self::sha256(bytes))
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for OciDigest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for OciDigest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ImagePlatform {
    pub os: String,
    pub architecture: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ContainerMode {
    Service,
    Job,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum RestartPolicy {
    Never,
    OnFailure,
    Always,
}

impl RestartPolicy {
    /// Only process termination drives this policy. Readiness is independent.
    pub fn restarts_after(self, exit_code: Option<i32>) -> bool {
        match self {
            Self::Never => false,
            Self::OnFailure => exit_code != Some(0),
            Self::Always => true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ContainerResources {
    pub cpu_millicores: u64,
    pub memory_bytes: u64,
    pub scratch_bytes: u64,
    /// Linux tasks, including threads and commands started through exec.
    pub pids_max: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PortProtocol {
    Http,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PortExposure {
    Public,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)
)]
pub enum ReadinessProbe {
    Tcp(TcpProbe),
    Http(HttpProbe),
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct TcpProbe {
    pub timeout_ms: u32,
    pub interval_ms: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct HttpProbe {
    pub path: String,
    pub timeout_ms: u32,
    pub interval_ms: u32,
}

impl Default for ReadinessProbe {
    fn default() -> Self {
        Self::Tcp(TcpProbe {
            timeout_ms: 1000,
            interval_ms: 5000,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ContainerPort {
    pub name: String,
    pub port: u16,
    pub protocol: PortProtocol,
    /// Required in the public input, with no implicit exposure default.
    pub exposure: PortExposure,
    #[cfg_attr(feature = "serde", serde(default))]
    pub readiness_probe: ReadinessProbe,
}

/// A placeholder declaration is never admitted until the mount protocol ships.
/// Keeping explicit declarations lets Stage 1 return an unsupported-feature error.
#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ContainerMount {
    pub database: String,
    pub source: String,
    pub target: String,
    pub read_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ContainerSpec {
    pub image_manifest: OciDigest,
    pub image_platform: ImagePlatform,
    /// Effective complete argv after applying OCI Entrypoint/Cmd or override.
    pub argv: Vec<String>,
    pub user: String,
    pub working_directory: String,
    pub mode: ContainerMode,
    pub restart: RestartPolicy,
    pub env_keys: Vec<String>,
    pub resources: ContainerResources,
    pub ports: Vec<ContainerPort>,
    pub mounts: Vec<ContainerMount>,
    pub stop_grace_ms: u32,
}

/// Explicit component removal is distinct from omission or an empty replacement.
#[derive(Clone, Debug, Default, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "action", content = "value", rename_all = "snake_case", deny_unknown_fields)
)]
#[expect(
    clippy::large_enum_variant,
    reason = "the normalized publish request owns its single container spec"
)]
pub enum ContainerAction {
    #[default]
    Keep,
    Set(ContainerSpec),
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid container {field}: {reason}")]
pub struct ContainerValidationError {
    pub field: &'static str,
    pub reason: &'static str,
}

fn invalid(field: &'static str, reason: &'static str) -> ContainerValidationError {
    ContainerValidationError { field, reason }
}

/// Limits are operational configuration; normalized specs are checked again at
/// admission against the selected node's enforceable capacities and capabilities.
#[derive(Clone, Copy, Debug)]
pub struct ContainerSpecLimits {
    pub resources: ContainerResources,
}

impl Default for ContainerSpecLimits {
    fn default() -> Self {
        Self {
            resources: ContainerResources {
                cpu_millicores: 64_000,
                memory_bytes: 128 * 1024 * 1024 * 1024,
                scratch_bytes: 1024 * 1024 * 1024 * 1024,
                pids_max: 4096,
            },
        }
    }
}

impl ContainerSpec {
    /// Sort semantically unordered declarations so equivalent specs hash alike.
    /// Duplicates are rejected, never silently deduplicated.
    pub fn normalize(mut self, limits: &ContainerSpecLimits) -> Result<Self, ContainerValidationError> {
        self.validate(limits)?;
        self.env_keys.sort_unstable();
        self.ports.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        Ok(self)
    }

    pub fn validate(&self, limits: &ContainerSpecLimits) -> Result<(), ContainerValidationError> {
        if self.image_platform.os != "linux" || !matches!(self.image_platform.architecture.as_str(), "amd64" | "arm64")
        {
            return Err(invalid("image_platform", "expected linux/amd64 or linux/arm64"));
        }
        if !self.mounts.is_empty() {
            return Err(invalid("mounts", "SpacetimeFS mounts are not supported by Stage 1"));
        }
        if self.mode == ContainerMode::Job && self.restart == RestartPolicy::Always {
            return Err(invalid("restart", "jobs cannot use the always restart policy"));
        }
        if self.argv.is_empty() || self.argv.len() > MAX_ARGV_ENTRIES || self.argv[0].is_empty() {
            return Err(invalid(
                "argv",
                "a nonempty command with at most 256 arguments is required",
            ));
        }
        validate_exec_size(&self.argv, &[])?;
        if self.user.len() > 255 || self.user.bytes().any(|b| b == 0 || b.is_ascii_control()) {
            return Err(invalid(
                "user",
                "user must fit 255 bytes and contain no control characters",
            ));
        }
        if !self.working_directory.starts_with('/')
            || self.working_directory.len() > 4096
            || self.working_directory.contains('\0')
        {
            return Err(invalid(
                "working_directory",
                "expected an absolute Linux path of at most 4096 bytes",
            ));
        }
        if self.stop_grace_ms > MAX_STOP_GRACE_MS {
            return Err(invalid("stop_grace_ms", "stop grace exceeds the supported deadline"));
        }
        let r = self.resources;
        let max = limits.resources;
        if r.cpu_millicores == 0
            || r.cpu_millicores > max.cpu_millicores
            || r.memory_bytes == 0
            || r.memory_bytes > max.memory_bytes
            || r.scratch_bytes == 0
            || r.scratch_bytes > max.scratch_bytes
            || r.pids_max == 0
            || r.pids_max > max.pids_max
        {
            return Err(invalid(
                "resources",
                "resource reservations must be positive and within server limits",
            ));
        }
        if self.env_keys.len() > MAX_ENV_KEYS {
            return Err(invalid("env_keys", "too many environment keys"));
        }
        let mut keys = BTreeSet::new();
        for key in &self.env_keys {
            validate_env_key(key)?;
            if !keys.insert(key) {
                return Err(invalid("env_keys", "duplicate environment key"));
            }
        }
        if self.ports.len() > MAX_PORTS {
            return Err(invalid("ports", "too many declared ports"));
        }
        let (mut names, mut numbers) = (BTreeSet::new(), BTreeSet::new());
        for port in &self.ports {
            let bytes = port.name.as_bytes();
            if bytes.is_empty()
                || bytes.len() > 32
                || !bytes[0].is_ascii_lowercase()
                || !bytes
                    .iter()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
            {
                return Err(invalid("ports.name", "expected [a-z][a-z0-9-]{0,31}"));
            }
            if port.port == 0 || !names.insert(&port.name) || !numbers.insert(port.port) {
                return Err(invalid("ports", "port names and nonzero port numbers must be unique"));
            }
            let (timeout, interval) = match &port.readiness_probe {
                ReadinessProbe::Tcp(TcpProbe {
                    timeout_ms,
                    interval_ms,
                }) => (*timeout_ms, *interval_ms),
                ReadinessProbe::Http(HttpProbe {
                    path,
                    timeout_ms,
                    interval_ms,
                }) => {
                    if !path.starts_with('/')
                        || path.starts_with("//")
                        || path.len() > 2048
                        || path
                            .bytes()
                            .any(|b| b.is_ascii_control() || b == b' ' || b == b'\\' || b == b'#')
                    {
                        return Err(invalid(
                            "readiness_probe.path",
                            "expected a bounded origin-relative HTTP path",
                        ));
                    }
                    (*timeout_ms, *interval_ms)
                }
            };
            if timeout == 0 || timeout > 30_000 || interval == 0 || interval > 300_000 || timeout > interval {
                return Err(invalid("readiness_probe", "invalid probe timeout or interval"));
            }
        }
        Ok(())
    }

    /// Domain-separated, versioned BSATN encoding, after normalization.
    /// This hashes the container spec only; the full deployment also includes
    /// the module selection and uses its own revision domain.
    pub fn canonical_hash(&self, limits: &ContainerSpecLimits) -> Result<Hash, ContainerValidationError> {
        let normalized = self.clone().normalize(limits)?;
        let encoded = bsatn::to_vec(&(CONTAINER_SPEC_VERSION, normalized))
            .expect("encoding an in-memory container specification cannot fail");
        let mut bytes = b"spacetimedb/container-spec\0".to_vec();
        bytes.extend(encoded);
        Ok(hash_bytes(&bytes))
    }
}

pub fn validate_env_key(key: &str) -> Result<(), ContainerValidationError> {
    let bytes = key.as_bytes();
    if bytes.is_empty()
        || bytes.len() > 256
        || !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_')
        || !bytes.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        return Err(invalid("env_keys", "invalid POSIX environment variable name"));
    }
    if key.starts_with("SPACETIMEDB_") {
        return Err(invalid(
            "env_keys",
            "SPACETIMEDB_ variables are reserved for the platform",
        ));
    }
    Ok(())
}

/// Call after merging image environment, the database snapshot, and platform
/// variables. Error messages deliberately contain neither argv nor env values.
pub fn validate_exec_size(argv: &[String], env: &[String]) -> Result<(), ContainerValidationError> {
    let count = argv
        .len()
        .checked_add(env.len())
        .and_then(|n| n.checked_add(2))
        .ok_or_else(|| invalid("exec", "too many arguments or environment entries"))?;
    let mut total = count
        .checked_mul(8)
        .and_then(|n| n.checked_add(EXEC_RESERVED_BYTES))
        .ok_or_else(|| invalid("exec", "argument and environment size overflow"))?;
    for value in argv.iter().chain(env) {
        if value.contains('\0') || value.len() >= MAX_EXEC_STRING_BYTES {
            return Err(invalid(
                "exec",
                "argument or environment entry contains NUL or exceeds the per-entry limit",
            ));
        }
        total = total
            .checked_add(value.len() + 1)
            .ok_or_else(|| invalid("exec", "argument and environment size overflow"))?;
    }
    if total > MAX_EXEC_BYTES {
        return Err(invalid(
            "exec",
            "combined argument and environment size exceeds the startup limit",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
