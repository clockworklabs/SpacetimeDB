//! Per-database declarations. Values and build credentials are never part of this configuration.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use spacetimedb_lib::container::{
    ContainerMode, ContainerMount, ContainerPort, ContainerResources, ContainerSpec, ContainerSpecLimits,
    ImagePlatform, OciDigest, RestartPolicy,
};
use spacetimedb_lib::environment::{EnvironmentConstraint, EnvironmentDeclaration, EnvironmentSchema, MAX_ENV_VARS};
use spacetimedb_oci::ContainerConfig as ImageConfig;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerConfig {
    pub image: ImageSource,
    pub command: Option<Vec<String>>,
    pub user: Option<String>,
    pub working_directory: Option<String>,
    #[serde(default = "default_mode")]
    pub mode: ContainerMode,
    #[serde(default = "default_restart")]
    pub restart: RestartPolicy,
    #[serde(default)]
    pub env_keys: Vec<String>,
    /// Declaration schema for a database without a user module. Values are
    /// supplied by the ordinary publish env configuration and shell override.
    #[serde(default)]
    pub env_schema: Option<BTreeMap<String, EnvironmentDeclarationConfig>>,
    pub resources: ContainerResources,
    #[serde(default)]
    pub ports: Vec<ContainerPort>,
    #[serde(default)]
    pub mounts: Vec<ContainerMount>,
    #[serde(default = "default_grace")]
    pub stop_grace_ms: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentDeclarationConfig {
    #[serde(default)]
    pub optional: bool,
    /// Omission accepts any string. One value is a literal constraint; several
    /// values are a finite union. An empty list is invalid.
    pub values: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ImageSource {
    Build(BuildImage),
    Prebuilt(PrebuiltImage),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildImage {
    #[serde(deserialize_with = "source_build")]
    pub build: SourceBuild,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrebuiltImage {
    pub oci_ref: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "builder", rename_all = "lowercase", deny_unknown_fields)]
pub enum SourceBuild {
    Dockerfile {
        #[serde(default = "default_context")]
        context: PathBuf,
        #[serde(default = "default_dockerfile")]
        dockerfile: PathBuf,
    },
    Railpack {
        #[serde(default = "default_context")]
        context: PathBuf,
    },
}

// A missing builder is Dockerfile. Keep deserialization strict after applying
// that one default, including rejection of Dockerfile-only fields on Railpack.
fn source_build<'de, D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<SourceBuild, D::Error> {
    let mut value = serde_json::Value::deserialize(deserializer)?;
    if let Some(object) = value.as_object_mut() {
        object.entry("builder").or_insert_with(|| "dockerfile".into());
    }
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}
fn default_context() -> PathBuf {
    PathBuf::from(".")
}
fn default_dockerfile() -> PathBuf {
    PathBuf::from("Dockerfile")
}
fn default_mode() -> ContainerMode {
    ContainerMode::Service
}
fn default_restart() -> RestartPolicy {
    RestartPolicy::OnFailure
}
fn default_grace() -> u32 {
    30_000
}

impl ContainerConfig {
    pub fn environment_schema(&self) -> Result<Option<EnvironmentSchema>> {
        let Some(schema) = &self.env_schema else {
            return Ok(None);
        };
        ensure!(
            schema.len() <= MAX_ENV_VARS,
            "too many container environment declarations"
        );
        let declarations = schema
            .iter()
            .map(|(name, declaration)| EnvironmentDeclaration {
                name: name.clone(),
                optional: declaration.optional,
                constraint: match declaration.values.as_deref() {
                    None => EnvironmentConstraint::AnyString,
                    Some([literal]) => EnvironmentConstraint::Literal(literal.clone()),
                    Some(values) => EnvironmentConstraint::OneOf(values.to_vec()),
                },
            })
            .collect();
        Ok(Some(EnvironmentSchema::new(declarations)?))
    }

    pub fn normalize(
        &self,
        manifest: OciDigest,
        platform: ImagePlatform,
        image: &ImageConfig,
    ) -> Result<ContainerSpec> {
        ensure!(self.mounts.is_empty(), "container mounts are not supported in Stage 1");
        self.environment_schema()?;
        let argv = image.argv(self.command.as_deref())?;
        spacetimedb_lib::container::validate_exec_size(&argv, image.env.as_deref().unwrap_or_default())?;
        Ok(ContainerSpec {
            image_manifest: manifest,
            image_platform: platform,
            argv,
            user: self.user.clone().unwrap_or_else(|| image.user.clone()),
            working_directory: self.working_directory.clone().unwrap_or_else(|| {
                if image.working_directory.is_empty() {
                    "/".into()
                } else {
                    image.working_directory.clone()
                }
            }),
            mode: self.mode,
            restart: self.restart,
            env_keys: self.env_keys.clone(),
            resources: self.resources,
            ports: self.ports.clone(),
            mounts: vec![],
            stop_grace_ms: self.stop_grace_ms,
        }
        .normalize(&ContainerSpecLimits::default())?)
    }
}
