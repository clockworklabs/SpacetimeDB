//! The container attached to a database, as exchanged between clients and the server.

use crate::environment::{validate_key, MAX_ENV_VARS};

pub const MAX_IMAGE_REF_BYTES: usize = 512;
pub const MAX_COMMAND_ARGS: usize = 256;
pub const MAX_COMMAND_BYTES: usize = 64 * 1024;
pub const MAX_PORTS: usize = 16;
pub const MAX_PORT_NAME_BYTES: usize = 63;
/// The largest image a server stores in a database: the gzipped output of `docker save`.
/// Stored images are table data, which lives in memory and goes into the commitlog.
pub const MAX_IMAGE_BYTES: u64 = 256 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ContainerPort {
    /// A DNS label naming the port, e.g. `http`.
    pub name: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ContainerResources {
    pub cpu_millicores: u32,
    pub memory_bytes: u64,
    /// Limit on the writable layer, including `/tmp`.
    pub scratch_bytes: u64,
    /// Limit on Linux tasks, including threads.
    pub pids_max: u32,
}

impl Default for ContainerResources {
    fn default() -> Self {
        Self {
            cpu_millicores: 1000,
            memory_bytes: 1 << 30,
            scratch_bytes: 1 << 30,
            pids_max: 512,
        }
    }
}

/// When the supervisor restarts a container's main command after it exits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum RestartPolicy {
    /// Restart after a nonzero exit or a kill; a zero exit completes the container.
    #[default]
    OnFailure,
    /// Restart after every exit.
    Always,
    /// Never restart.
    Never,
}

impl RestartPolicy {
    /// Whether a main command that exited this way should be started again.
    pub fn restarts(self, exit_code: i64, killed: bool) -> bool {
        match self {
            Self::OnFailure => exit_code != 0 || killed,
            Self::Always => true,
            Self::Never => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ContainerSpec {
    /// An OCI image reference pinned to a manifest digest: `name@sha256:<hex>`.
    /// Servers that accept them also take a local image ID, `sha256:<hex>`; see [`is_local_image_id`].
    pub image: String,
    /// Replaces the image's entrypoint and command when present.
    #[cfg_attr(feature = "serde", serde(default))]
    pub command: Option<Vec<String>>,
    /// Keys of the database environment to set in the container.
    #[cfg_attr(feature = "serde", serde(default))]
    pub env_keys: Vec<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub resources: ContainerResources,
    #[cfg_attr(feature = "serde", serde(default))]
    pub ports: Vec<ContainerPort>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub restart: RestartPolicy,
}

impl ContainerSpec {
    /// Check the spec for a server that pulls every image from a registry.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_with(false)
    }

    /// Check the spec. With `local_images`, the image may be a local image ID instead of a
    /// reference pinned to a digest.
    pub fn validate_with(&self, local_images: bool) -> Result<(), String> {
        validate_image(&self.image, local_images)?;
        if let Some(command) = &self.command {
            if command.is_empty() || command.len() > MAX_COMMAND_ARGS {
                return Err(format!("command must have 1 to {MAX_COMMAND_ARGS} arguments"));
            }
            if command.iter().map(|arg| arg.len()).sum::<usize>() > MAX_COMMAND_BYTES {
                return Err(format!("command exceeds {MAX_COMMAND_BYTES} bytes"));
            }
            if command.iter().any(|arg| arg.contains('\0')) {
                return Err("command arguments cannot contain NUL".into());
            }
        }
        if self.env_keys.len() > MAX_ENV_VARS {
            return Err(format!("at most {MAX_ENV_VARS} environment keys are allowed"));
        }
        for key in &self.env_keys {
            validate_key(key).map_err(|e| format!("environment key `{key}`: {e}"))?;
        }
        let r = &self.resources;
        if r.cpu_millicores == 0 || r.memory_bytes == 0 || r.scratch_bytes == 0 || r.pids_max == 0 {
            return Err("container resource limits must be nonzero".into());
        }
        if self.ports.len() > MAX_PORTS {
            return Err(format!("at most {MAX_PORTS} ports are allowed"));
        }
        for (i, port) in self.ports.iter().enumerate() {
            validate_port_name(&port.name)?;
            if port.port == 0 {
                return Err(format!("port `{}` must be nonzero", port.name));
            }
            if self.ports[..i]
                .iter()
                .any(|p| p.name == port.name || p.port == port.port)
            {
                return Err(format!("port `{}` is declared more than once", port.name));
            }
        }
        Ok(())
    }
}

/// Whether `image` is a local image ID, `sha256:<64 hex digits>`: the ID of an image in a Docker
/// daemon, which a server runs if its daemon has it or the database stores it, and never pulls.
pub fn is_local_image_id(image: &str) -> bool {
    image.strip_prefix("sha256:").is_some_and(is_sha256_hex)
}

fn is_sha256_hex(hex: &str) -> bool {
    hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn validate_image(image: &str, local_images: bool) -> Result<(), String> {
    if is_local_image_id(image) {
        return match local_images {
            true => Ok(()),
            false => Err(format!(
                "image `{image}` is a local image ID, which this server does not run; push the image to a \
                 registry and pass a reference pinned to a digest, like `name@sha256:<64 hex digits>`"
            )),
        };
    }
    let err = || {
        let or_local = if local_images {
            ", or a local image ID, like `sha256:<64 hex digits>`"
        } else {
            ""
        };
        format!("image `{image}` must be a reference pinned to a digest, like `name@sha256:<64 hex digits>`{or_local}")
    };
    if image.len() > MAX_IMAGE_REF_BYTES {
        return Err(err());
    }
    let (name, digest) = image.split_once('@').ok_or_else(err)?;
    let hex = digest.strip_prefix("sha256:").ok_or_else(err)?;
    if name.is_empty() || name.chars().any(|c| c.is_whitespace() || c.is_control()) || !is_sha256_hex(hex) {
        return Err(err());
    }
    Ok(())
}

fn validate_port_name(name: &str) -> Result<(), String> {
    let bytes = name.as_bytes();
    let valid = !bytes.is_empty()
        && bytes.len() <= MAX_PORT_NAME_BYTES
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        && bytes[0] != b'-'
        && bytes[bytes.len() - 1] != b'-';
    if valid {
        Ok(())
    } else {
        Err(format!("port name `{name}` must be a lowercase DNS label"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ContainerState {
    Starting,
    Running,
    /// The main command exited with this status.
    Exited(i64),
    /// The kernel killed the container for exceeding its memory limit.
    OutOfMemory,
    /// The container could not be started, for this reason.
    Failed(String),
}

/// The response to `GET /database/:name_or_identity/container`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ContainerInfo {
    pub spec: ContainerSpec,
    /// Whether the container should run.
    pub running: bool,
    pub generation: u64,
    /// The last state reported for `generation`, if any.
    pub state: Option<ContainerState>,
}

/// The response to `GET /database/:name_or_identity/container/platform`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ContainerPlatform {
    /// The platform the database's container runs on, like `linux/arm64`, which images stored in
    /// the database must be built for.
    pub platform: String,
    /// The largest image the database stores; see [`MAX_IMAGE_BYTES`].
    pub max_image_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ContainerSpec {
        ContainerSpec {
            image: format!("registry.local/agent@sha256:{}", "a".repeat(64)),
            command: None,
            env_keys: vec!["API_KEY".into()],
            resources: ContainerResources::default(),
            ports: vec![ContainerPort {
                name: "http".into(),
                port: 8080,
            }],
            restart: RestartPolicy::default(),
        }
    }

    #[test]
    fn accepts_a_pinned_spec() {
        assert_eq!(spec().validate(), Ok(()));
    }

    #[test]
    fn rejects_unpinned_images() {
        for image in ["agent:latest", "agent@sha256:abc", "@sha256:".to_string().as_str()] {
            let spec = ContainerSpec {
                image: image.into(),
                ..spec()
            };
            assert!(spec.validate().is_err(), "{image}");
        }
    }

    #[test]
    fn accepts_local_image_ids_only_when_allowed() {
        let local = ContainerSpec {
            image: format!("sha256:{}", "a".repeat(64)),
            ..spec()
        };
        assert!(local.validate().is_err());
        assert_eq!(local.validate_with(true), Ok(()));
        assert_eq!(spec().validate_with(true), Ok(()));
        for image in ["sha256:abc", "sha256:", "agent:latest"] {
            let spec = ContainerSpec {
                image: image.into(),
                ..spec()
            };
            assert!(spec.validate_with(true).is_err(), "{image}");
        }
    }

    #[test]
    fn rejects_invalid_keys_ports_and_limits() {
        let mut s = spec();
        s.env_keys.push("1BAD".into());
        assert!(s.validate().is_err());

        let mut s = spec();
        s.ports.push(ContainerPort {
            name: "http".into(),
            port: 9090,
        });
        assert!(s.validate().is_err());

        let mut s = spec();
        s.ports[0].name = "HTTP".into();
        assert!(s.validate().is_err());

        let mut s = spec();
        s.resources.memory_bytes = 0;
        assert!(s.validate().is_err());

        let mut s = spec();
        s.command = Some(vec![]);
        assert!(s.validate().is_err());
    }

    #[test]
    fn restart_policies() {
        use RestartPolicy::*;
        assert!(!OnFailure.restarts(0, false));
        assert!(OnFailure.restarts(1, false));
        assert!(OnFailure.restarts(0, true));
        assert!(Always.restarts(0, false));
        assert!(!Never.restarts(1, true));
    }
}
