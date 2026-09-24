//! Managed publication frontend. The resume journal owns every byte needed to
//! retry; project configuration and mutable image tags are only read initially.
use super::{confirm_major_version_upgrade, environment, YesFlags};
use crate::{
    common_args::ClearMode,
    config::Config,
    container::{
        self,
        oci::ArtifactKind,
        process::LocalRunner,
        publish::{
            self,
            client::{ObjectRef, PublisherClient, UploadKind},
            journal::{Journal, Record, UploadRecord},
            Outcome,
        },
        BuildSecret, BuildTools,
    },
    spacetime_config::{CommandConfig, CommandSchemaBuilder},
    util::{get_auth_header, y_or_n, AuthHeader},
};
use anyhow::{bail, ensure, Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use spacetimedb_client_api_messages::name::{is_identity, DomainName, PrePublishResult};
use spacetimedb_lib::{
    container::{ContainerAction, ImagePlatform},
    deployment::{self, api::*, manifest::*, ModuleAction, PublishEnvelope, UserModule, UserModuleKind},
    Identity, Uuid,
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

const ARGUMENTS: &[&str] = &[
    "managed",
    "container_platform",
    "artifact_endpoint",
    "remove_container",
    "remove_module",
    "publication_state_dir",
    "resume_publication",
    "publication_wait",
    "buildkit_host",
    "buildctl",
    "railpack",
    "skopeo",
    "registry_auth_file",
    "build_secret",
];
pub(super) fn exclude_args(mut schema: CommandSchemaBuilder) -> CommandSchemaBuilder {
    for arg in ARGUMENTS {
        schema = schema.exclude(*arg);
    }
    schema
}
pub(super) fn add_args(mut command: Command) -> Command {
    command = command
        .arg(
            Arg::new("managed")
                .long("managed")
                .action(ArgAction::SetTrue)
                .help("Use managed deployment publication, including for a module-only database"),
        )
        .arg(
            Arg::new("container_platform")
                .long("container-platform")
                .value_parser(["linux/amd64", "linux/arm64"])
                .help("Required platform when publishing a container declaration"),
        )
        .arg(
            Arg::new("artifact_endpoint")
                .long("artifact-endpoint")
                .help("Explicitly authorize this exact artifact URL to receive the publisher credential"),
        )
        .arg(
            Arg::new("remove_container")
                .long("remove-container")
                .action(ArgAction::SetTrue)
                .help("Explicitly remove the container while preserving the module unless separately changed"),
        )
        .arg(
            Arg::new("remove_module")
                .long("remove-module")
                .action(ArgAction::SetTrue)
                .conflicts_with_all(["module_path", "wasm_file", "js_file"])
                .help("Replace the module with the versioned empty module after migration preflight"),
        )
        .arg(
            Arg::new("publication_state_dir")
                .long("publication-state-dir")
                .value_parser(clap::value_parser!(PathBuf))
                .help("Private local directory retaining complete managed publication inputs and progress")
                .long_help("Private local directory retaining complete managed publication inputs and progress. The protected submission file includes resolved environment values. Keep this directory private; do not commit it or share it. Resume reuses these exact values without rereading project configuration or shell variables."),
        )
        .arg(
            Arg::new("resume_publication")
                .long("resume-publication")
                .value_parser(clap::value_parser!(PathBuf))
                .conflicts_with_all([
                    "managed",
                    "remove_container",
                    "remove_module",
                    "module_path",
                    "wasm_file",
                    "js_file",
                    "container_platform",
                    "name|identity",
                    "parent",
                    "organization",
                    "clear-database",
                ])
                .help("Resume this operation directory without rebuilding or reading spacetime.json"),
        )
        .arg(
            Arg::new("publication_wait")
                .long("publication-wait")
                .default_value("60")
                .value_parser(clap::value_parser!(u64).range(0..=3600))
                .help("Seconds to wait for managed activation; pending operations retain their resume directory"),
        )
        .arg(
            Arg::new("buildkit_host")
                .long("buildkit-host")
                .help("Explicit local BuildKit Unix socket for container source builds"),
        )
        .arg(
            Arg::new("registry_auth_file")
                .long("registry-auth-file")
                .value_parser(clap::value_parser!(PathBuf))
                .help("Explicit registry auth JSON; otherwise image preparation is anonymous"),
        )
        .arg(
            Arg::new("build_secret")
                .long("build-secret")
                .action(ArgAction::Append)
                .value_name("NAME=FILE")
                .help("Build secret file, separate from runtime env_keys"),
        );
    for (tool, help) in [
        (
            "buildctl",
            "Path to the buildctl executable for Dockerfile or Railpack builds",
        ),
        (
            "railpack",
            "Path to the Railpack executable for explicitly selected Railpack builds",
        ),
        (
            "skopeo",
            "Path to the skopeo executable for copying prebuilt OCI images",
        ),
    ] {
        command = command.arg(
            Arg::new(tool)
                .help(help)
                .long(tool)
                .default_value(tool)
                .value_parser(clap::value_parser!(PathBuf)),
        );
    }
    command
}
fn approved(args: &ArgMatches) -> Option<&str> {
    args.get_one::<String>("artifact_endpoint").map(String::as_str)
}
fn wait(args: &ArgMatches) -> Duration {
    Duration::from_secs(*args.get_one::<u64>("publication_wait").unwrap_or(&60))
}
pub(super) fn validate_target_options(target: &CommandConfig<'_>) -> Result<()> {
    let args = target.matches();
    if target.container().is_some()
        || args.get_flag("managed")
        || args.get_flag("remove_container")
        || args.get_flag("remove_module")
    {
        ensure!(
            !target.get_one::<bool>("anon_identity")?.unwrap_or(false),
            "managed publication requires an authenticated publisher"
        );
    }
    Ok(())
}
fn authenticated(server: &str, auth: &AuthHeader) -> Result<PublisherClient> {
    PublisherClient::new(
        server,
        auth.to_header()
            .context("managed publication requires an authenticated publisher; anonymous publication is unsupported")?,
    )
}
pub(super) async fn resume(config: &mut Config, args: &ArgMatches, yes: YesFlags) -> Result<()> {
    let selection = args.get_one::<String>("server").map(String::as_str);
    let server = config.get_host_url(selection)?;
    let mut journal = Journal::open(args.get_one::<PathBuf>("resume_publication").unwrap())?;
    ensure!(
        container::publish::client::endpoint(&server)?.as_str() == journal.record.server,
        "resume server differs from the original publication endpoint"
    );
    let auth = get_auth_header(config, false, selection, !yes.skip_login).await?;
    let client = authenticated(&server, &auth)?;
    execute(&client, &mut journal, args).await
}

/// False is only returned for an ordinary, un-managed module publication.
/// Once managed intent or a managed revision is known, errors cannot fall back.
#[allow(clippy::too_many_arguments)]
pub(super) async fn try_execute(
    target: &CommandConfig<'_>,
    config_dir: Option<&Path>,
    server: &str,
    auth: &AuthHeader,
    name: Option<&str>,
    parent: Option<&str>,
    clear: ClearMode,
    yes: YesFlags,
) -> Result<bool> {
    let args = target.matches();
    let explicit = target.container().is_some()
        || args.get_flag("managed")
        || args.get_flag("remove_container")
        || args.get_flag("remove_module");
    if !explicit && name.is_none() {
        return Ok(false);
    }
    let client = match authenticated(server, auth) {
        Ok(client) => client,
        Err(error) if explicit => return Err(error),
        Err(_) => return Ok(false),
    };
    try_execute_with_client(target, config_dir, client, name, parent, clear, yes).await
}

async fn try_execute_with_client(
    target: &CommandConfig<'_>,
    config_dir: Option<&Path>,
    client: PublisherClient,
    name: Option<&str>,
    parent: Option<&str>,
    clear: ClearMode,
    yes: YesFlags,
) -> Result<bool> {
    let args = target.matches();
    let explicit = target.container().is_some()
        || args.get_flag("managed")
        || args.get_flag("remove_container")
        || args.get_flag("remove_module");
    let caps = client.capabilities().await?;
    let Some(caps) = caps else {
        ensure!(
            !explicit,
            "this server does not support managed publication; no module-only fallback was attempted"
        );
        return Ok(false);
    };
    let prior = if let Some(name) = name {
        client.deployment(name).await?
    } else {
        None
    };
    if !explicit && prior.as_ref().and_then(|p| p.revision).is_none() {
        return Ok(false);
    }
    ensure!(
        caps.enabled && caps.version == deployment::PUBLISH_PROTOCOL_VERSION,
        "managed publication is disabled or incompatible on this server"
    );
    ensure!(
        clear == ClearMode::Never,
        "managed publication does not support --delete-data; resolve migrations without replacing database storage"
    );
    ensure!(
        !(target.container().is_some() && args.get_flag("remove_container")),
        "a container declaration and --remove-container cannot both select this target"
    );
    if prior.is_none() {
        ensure!(
            !name.is_some_and(is_identity),
            "a new database Identity must be generated by the reservation service; select a name or omit DATABASE"
        );
    }
    let artifact = client.artifact_endpoint(
        caps.artifact_endpoint
            .as_deref()
            .context("server did not advertise an artifact endpoint")?,
        approved(args),
    )?;
    let permission = client.permission().await?;
    if target.container().is_some() || prior.is_none() {
        ensure!(
            permission.can_publish,
            "the current publisher does not have container publication permission"
        );
    }
    if !container::publish::client::is_loopback(client.server()) {
        ensure!(
            y_or_n(
                yes.publish_to_remote,
                "Publish this managed deployment to the selected remote server?"
            )?,
            "publication cancelled"
        );
    }
    let cwd = std::env::current_dir()?;
    let base = args
        .get_one::<PathBuf>("publication_state_dir")
        .cloned()
        .unwrap_or_else(|| config_dir.unwrap_or(&cwd).join(".spacetime/publications"));
    let _base_parents = Journal::prepare_base(&base)?;
    let cancel = CancellationToken::new();
    let prepare = prepare_request(
        &client,
        target,
        config_dir.unwrap_or(&cwd),
        &base,
        prior.as_ref(),
        name,
        parent,
        permission.identity,
        artifact.as_str(),
        yes,
        cancel.clone(),
    );
    tokio::pin!(prepare);
    let mut journal = tokio::select! {
        result = &mut prepare => result?,
        signal = tokio::signal::ctrl_c() => {
            signal?; cancel.cancel(); let _ = prepare.await;
            bail!("managed preparation cancelled before admission");
        }
    };
    execute(&client, &mut journal, args).await?;
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
async fn prepare_request(
    client: &PublisherClient,
    target: &CommandConfig<'_>,
    config_dir: &Path,
    base: &Path,
    prior: Option<&DeploymentStatus>,
    name: Option<&str>,
    parent: Option<&str>,
    publisher: Identity,
    artifact_endpoint: &str,
    yes: YesFlags,
    cancel: CancellationToken,
) -> Result<Journal> {
    let args = target.matches();
    let environment_options = super::EnvironmentOptions::from_args(args)?;
    let environment_only = environment_options.only;
    ensure!(
        !environment_only || prior.is_some(),
        "--env-only requires an existing database"
    );
    ensure!(
        !environment_only || (!args.get_flag("remove_module") && !args.get_flag("remove_container")),
        "--env-only cannot remove a module or container"
    );
    let image = if let Some(declaration) = target.container().filter(|_| !environment_only) {
        let (os, architecture) = args
            .get_one::<String>("container_platform")
            .context("publishing a container requires --container-platform linux/amd64 or linux/arm64")?
            .split_once('/')
            .unwrap();
        Some(
            container::prepare_container(
                declaration,
                config_dir,
                ImagePlatform {
                    os: os.into(),
                    architecture: architecture.into(),
                },
                &tools(args)?,
                base,
                &LocalRunner,
                cancel.clone(),
            )
            .await?,
        )
    } else {
        None
    };
    let has_module = !environment_only
        && ["module_path", "wasm_file", "js_file"]
            .iter()
            .any(|key| target.is_from_cli(key) || target.get_config_value(key).is_some());
    ensure!(
        !(has_module && args.get_flag("remove_module")),
        "module configuration and --remove-module cannot both select this target"
    );
    let module = if !environment_only
        && !args.get_flag("remove_module")
        && (has_module || (target.container().is_none() && !args.get_flag("remove_container")))
    {
        Some(load_module(target, config_dir).await?)
    } else {
        None
    };
    let declared_environment = target
        .container()
        .filter(|_| !environment_only)
        .map(|config| config.environment_schema())
        .transpose()?
        .flatten();
    ensure!(
        declared_environment.is_none() || module.is_none(),
        "container env_schema cannot override a user module's environment declarations"
    );
    ensure!(
        declared_environment.is_none()
            || args.get_flag("remove_module")
            || !prior
                .is_some_and(|prior| matches!(prior.deployment.current().module, deployment::ModuleComponent::User(_))),
        "replacing a user module with container env_schema requires --remove-module"
    );
    let builtin =
        if module.is_none() && (args.get_flag("remove_module") || declared_environment.is_some() || prior.is_none()) {
            Some(deployment::system_empty::generate(
                &declared_environment.unwrap_or_default(),
            )?)
        } else {
            None
        };
    let module_action = if let Some(builtin) = &builtin {
        ModuleAction::Remove(builtin.descriptor)
    } else if let Some((kind, bytes)) = &module {
        ModuleAction::Set(UserModule {
            kind: *kind,
            program_hash: spacetimedb_lib::hash_bytes(bytes),
        })
    } else {
        ModuleAction::Keep
    };
    let module = builtin
        .map(|builtin| (UserModuleKind::Wasm, builtin.bytes.into_vec()))
        .or(module);
    let container_action = if let Some(image) = &image {
        ContainerAction::Set(image.metadata.container.clone())
    } else if args.get_flag("remove_container") {
        ContainerAction::Remove
    } else {
        ContainerAction::Keep
    };
    let envelope = PublishEnvelope {
        version: deployment::PUBLISH_PROTOCOL_VERSION,
        operation_id: Uuid::from_u128(uuid::Uuid::now_v7().as_u128()),
        expected_revision: prior.and_then(|p| p.revision),
        expected_last_operation: prior.and_then(|p| p.last_operation),
        module_action,
        container_action,
    };
    let deployment = envelope.resolve(prior.map(|p| &p.deployment), &Default::default())?;
    let module_artifact = if let Some((_, bytes)) = &module {
        ModuleArtifact {
            digest: spacetimedb_oci::sha256(bytes),
            size_bytes: bytes.len() as u64,
        }
    } else if let Some(prior) = prior {
        let artifact = prior.module_artifact;
        ModuleArtifact {
            digest: artifact.digest,
            size_bytes: artifact.size_bytes,
        }
    } else {
        bail!("publication has no selected module artifact")
    };
    // Resolve supplied overrides without downloading stored secret values.
    // For Keep, authenticated schema metadata is tied to the observed program;
    // a changed committed operation is rejected by the later pair CAS.
    let schema = if let Some((kind, bytes)) = &module {
        selected_environment(*kind, bytes).await?
    } else {
        client
            .selected_environment(prior.context("publication has no selected module")?)
            .await?
    };
    let environment = environment::resolve(&schema, target.get_config_value("env"), |key| std::env::var_os(key))?;
    environment_options.validate_values(&environment.values)?;
    print!("{}", environment.display());
    let migration_policy = if let Some(prior) = prior {
        if let Some((kind, bytes)) = &module {
            migration(client, prior.database_identity, *kind, bytes, target, yes).await?
        } else {
            PreparedMigrationPolicy::Compatible
        }
    } else {
        PreparedMigrationPolicy::Compatible
    };
    let creation = if prior.is_none() {
        let parent = if let Some(parent) = parent {
            Some(
                client
                    .deployment(parent)
                    .await?
                    .context("parent database does not exist or is not accessible")?
                    .database_identity,
            )
        } else {
            None
        };
        let organization = target
            .get_one::<String>("organization")?
            .map(|value| {
                value
                    .parse::<Identity>()
                    .context("managed publication currently requires --organization IDENTITY, not an organization name")
            })
            .transpose()?;
        Some(CreationOptions {
            parent,
            organization,
            num_replicas: target.get_one::<u8>("num_replicas")?.map(u32::from),
            enforce_anti_affinity: true,
        })
    } else {
        None
    };
    let request = PublishRequest {
        environment: environment.values,
        environment_remove: environment_options.remove,
        environment_replace: environment_options.replace,
        manifest: PreparedDeploymentManifest::V1(PreparedDeploymentManifestV1 {
            envelope,
            deployment,
            module_artifact,
            migration_policy,
        }),
        creation,
        image_source: image.as_ref().map(|image| ArtifactReference {
            digest: image.metadata.manifest.digest,
            size_bytes: image.metadata.manifest.size,
        }),
    };
    request.manifest.validate(&Default::default())?;
    let request_json = serde_json::to_string(&request)?;
    let mut uploads = image
        .as_ref()
        .map(|image| {
            image
                .metadata
                .objects
                .iter()
                .map(|object| UploadRecord {
                    kind: match object.kind {
                        ArtifactKind::Manifest => UploadKind::Manifest,
                        ArtifactKind::Config => UploadKind::Config,
                        ArtifactKind::Layer => UploadKind::Layer,
                    },
                    object: ObjectRef {
                        digest: object.descriptor.digest,
                        size: object.descriptor.size,
                    },
                    session: None,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if module.is_some() {
        uploads.push(UploadRecord {
            kind: UploadKind::Module,
            object: ObjectRef {
                digest: module_artifact.digest,
                size: module_artifact.size_bytes,
            },
            session: None,
        });
    }
    let requested_name = if prior.is_none() {
        name.map(|name| name.parse::<DomainName>().map(|name| name.to_string()))
            .transpose()?
    } else {
        None
    };
    let record = Record {
        version: 2,
        server: client.server().to_string(),
        artifact_endpoint: artifact_endpoint.into(),
        publisher,
        database: prior.map(|p| p.database_identity),
        reservation: request.creation.clone().map(|options| ReserveDatabaseRequest {
            version: deployment::PUBLISH_PROTOCOL_VERSION,
            operation_id: request.manifest.current().envelope.operation_id,
            options,
        }),
        requested_name,
        request_digest: spacetimedb_oci::sha256(request_json.as_bytes()),
        request_json,
        uploads,
        submitted: false,
        status: None,
        name_confirmed: false,
        naming_attempted: false,
    };
    Journal::create(base, record, image, module.as_ref().map(|(_, bytes)| bytes.as_slice()))
}
async fn selected_environment(
    kind: UserModuleKind,
    bytes: &[u8],
) -> Result<spacetimedb_lib::environment::EnvironmentSchema> {
    // Canonical platform modules can be inspected without executing a helper.
    // The verifier regenerates every byte; this is equally valid if a user
    // explicitly selected those exact canonical Wasm bytes with --bin-path.
    if kind == UserModuleKind::Wasm {
        let descriptor = deployment::system_empty::SystemEmptyModule {
            version: deployment::system_empty::VERSION,
            program_hash: spacetimedb_lib::hash_bytes(bytes),
        };
        if let Ok(schema) = deployment::system_empty::verify(&descriptor, bytes) {
            return Ok(schema);
        }
    }
    let host_type = match kind {
        UserModuleKind::Wasm => "Wasm",
        UserModuleKind::Js => "Js",
    };
    Ok(environment::inspect(bytes, host_type).await?.environment().clone())
}

fn tools(args: &ArgMatches) -> Result<BuildTools> {
    let mut tools = BuildTools {
        buildctl: args.get_one::<PathBuf>("buildctl").unwrap().clone(),
        railpack: args.get_one::<PathBuf>("railpack").unwrap().clone(),
        skopeo: args.get_one::<PathBuf>("skopeo").unwrap().clone(),
        buildkit_host: args.get_one::<String>("buildkit_host").cloned(),
        registry_auth_file: args.get_one::<PathBuf>("registry_auth_file").cloned(),
        secrets: vec![],
    };
    for value in args.get_many::<String>("build_secret").into_iter().flatten() {
        let (name, file) = value.split_once('=').context("build secret must be NAME=FILE")?;
        ensure!(!file.is_empty(), "build secret file is missing");
        tools.secrets.push(BuildSecret {
            name: name.into(),
            file: file.into(),
        });
    }
    Ok(tools)
}
async fn load_module(target: &CommandConfig<'_>, config_dir: &Path) -> Result<(UserModuleKind, Vec<u8>)> {
    let native_aot = target.get_one::<bool>("native_aot")?.unwrap_or(false);
    ensure!(
        !native_aot,
        "managed NativeAOT builds are not yet supported; build separately and pass --bin-path"
    );
    let (path, kind) = if let Some(path) = target.get_resolved_path("wasm_file", Some(config_dir))? {
        (path, "Wasm")
    } else if let Some(path) = target.get_resolved_path("js_file", Some(config_dir))? {
        (path, "Js")
    } else {
        let path = target
            .get_resolved_path("module_path", Some(config_dir))?
            .unwrap_or_else(|| super::default_publish_module_path(config_dir));
        crate::build::exec_with_argstring(
            &path,
            &target.get_one::<String>("build_options")?.unwrap_or_default(),
            native_aot,
            super::dotnet_version_from_config(target)?,
        )
        .await?
    };
    let kind = match kind {
        "Wasm" => UserModuleKind::Wasm,
        "Js" => UserModuleKind::Js,
        _ => bail!("unsupported managed module kind"),
    };
    let metadata = tokio::fs::metadata(&path).await?;
    ensure!(
        metadata.is_file() && metadata.len() > 0 && metadata.len() <= MAX_MODULE_ARTIFACT_BYTES,
        "module must be a regular file of at most 32 MiB"
    );
    use tokio::io::AsyncReadExt as _;
    let mut bytes = Vec::new();
    tokio::fs::File::open(&path)
        .await?
        .take(MAX_MODULE_ARTIFACT_BYTES + 1)
        .read_to_end(&mut bytes)
        .await?;
    ensure!(
        !bytes.is_empty() && bytes.len() as u64 <= MAX_MODULE_ARTIFACT_BYTES,
        "module exceeds 32 MiB"
    );
    Ok((kind, bytes))
}
async fn migration(
    client: &PublisherClient,
    database: Identity,
    kind: UserModuleKind,
    bytes: &[u8],
    target: &CommandConfig<'_>,
    yes: YesFlags,
) -> Result<PreparedMigrationPolicy> {
    let pre = client
        .preflight(
            database,
            match kind {
                UserModuleKind::Wasm => "Wasm",
                UserModuleKind::Js => "Js",
            },
            bytes,
        )
        .await?;
    match pre {
        PrePublishResult::ManualMigrate(_) => {
            bail!("managed publication requires manual migration; no storage was cleared")
        }
        PrePublishResult::AutoMigrate(auto) => {
            if auto.major_version_upgrade {
                confirm_major_version_upgrade(yes.migrate_major_version)?;
            }
            println!("{}", auto.migrate_plan);
            if auto.break_clients {
                ensure!(
                    y_or_n(
                        yes.break_clients || target.get_one::<bool>("break_clients")?.unwrap_or(false),
                        "These changes will BREAK existing clients. Proceed?"
                    )?,
                    "publication cancelled"
                );
            }
            Ok(PreparedMigrationPolicy::BreakClients(auto.token))
        }
    }
}
async fn execute(client: &PublisherClient, journal: &mut Journal, args: &ArgMatches) -> Result<()> {
    println!(
        "Publication {}. Resume directory: {}",
        journal.operation()?,
        journal.directory().display()
    );
    let cancel = CancellationToken::new();
    let operation = publish::run(client, journal, approved(args), wait(args), cancel.clone());
    tokio::pin!(operation);
    let outcome = tokio::select! {
        result = &mut operation => result?,
        signal = tokio::signal::ctrl_c() => { signal?; cancel.cancel(); bail!("publication interrupted; resume the same directory to determine its outcome"); }
    };
    match outcome {
        Outcome::Complete(status) => println!(
            "Deployment {} is active on {}",
            status.proposed_revision, status.database_identity
        ),
        Outcome::Pending(status) => println!(
            "Publication {} is {:?} on {}; resume to observe activation",
            status.operation_id, status.phase, status.database_identity
        ),
        Outcome::Aborted(status) => bail!(
            "publication {} was aborted before commit on {}; resume state retained",
            status.operation_id,
            status.database_identity
        ),
        Outcome::NamingUnconfirmed(status) => bail!(
            "deployment is active on {}, but naming was not confirmed; publication must not be repeated",
            status.database_identity
        ),
    }
    Ok(())
}

#[cfg(test)]
mod environment_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::publish::tests::{database, Fixture};
    use crate::spacetime_config::SpacetimeConfig;
    use serde_json::json;
    use spacetimedb_paths::FromPathUnchecked as _;
    use std::collections::HashMap;

    #[tokio::test]
    async fn container_only_frontend_uploads_verified_closure_with_server_reserved_identity() {
        let fixture = Fixture::new().await;
        let temporary = crate::container::publish::tests::temporary_directory();
        let layout = temporary.path().join("layout");
        let selected = crate::container::tests::fixture(&layout);
        let declaration = crate::container::tests::declaration(json!({"oci_ref":"oci:layout"}));
        let command = super::super::cli();
        let schema = super::super::build_publish_schema(&command).unwrap();
        let state = temporary.path().join("new/state");
        let args = command
            .try_get_matches_from([
                "publish",
                "fixture-name",
                "--server",
                fixture.endpoint.as_str(),
                "--container-platform",
                "linux/amd64",
                "--publication-state-dir",
                state.to_str().unwrap(),
                "--publication-wait",
                "0",
            ])
            .unwrap();
        let target = CommandConfig::new(&schema, HashMap::new(), &args)
            .unwrap()
            .with_container(Some(declaration));
        assert!(try_execute_with_client(
            &target,
            Some(temporary.path()),
            fixture.client(),
            Some("fixture-name"),
            None,
            ClearMode::Never,
            YesFlags::all()
        )
        .await
        .unwrap());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for parent in [temporary.path().join("new"), state.clone()] {
                assert_eq!(std::fs::metadata(parent).unwrap().permissions().mode() & 0o777, 0o700);
            }
        }
        {
            let snapshot = fixture.state.lock().unwrap();
            assert_eq!(snapshot.reservations.len(), 1);
            assert_eq!(snapshot.submits.len(), 1);
            assert_eq!(snapshot.begin_count, 4);
            let request: PublishRequest = serde_json::from_slice(&snapshot.submits[0]).unwrap();
            assert!(matches!(
                request.manifest.current().envelope.module_action,
                ModuleAction::Remove(_)
            ));
            assert_eq!(
                request.manifest.current().deployment.current().module,
                deployment::ModuleComponent::SystemEmpty(deployment::system_empty::empty().descriptor)
            );
            assert_eq!(
                request.manifest.current().module_artifact,
                crate::container::publish::tests::empty_module_artifact()
            );
            assert_eq!(request.image_source.unwrap().digest, selected.digest);
            let dir = state.join(request.manifest.current().envelope.operation_id.to_string());
            let journal = Journal::open(&dir).unwrap();
            assert_eq!(journal.record.database, Some(database()));
            let bytes = std::fs::read_to_string(dir.join("publication.json")).unwrap();
            assert!(!bytes.contains("private-image-value"));
        }
        fixture.close().await;
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn writable_publication_parent_is_rejected_before_image_preparation_or_submission() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new().await;
        let temporary = crate::container::publish::tests::temporary_directory();
        let state = temporary.path().join("state");
        std::fs::create_dir(&state).unwrap();
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o777)).unwrap();
        let command = super::super::cli();
        let schema = super::super::build_publish_schema(&command).unwrap();
        let args = command
            .try_get_matches_from([
                "publish",
                "fixture-name",
                "--server",
                fixture.endpoint.as_str(),
                "--container-platform",
                "linux/amd64",
                "--publication-state-dir",
                state.to_str().unwrap(),
            ])
            .unwrap();
        // If preparation runs, this missing local source would fail first.
        // No real image helper, registry, or saved configuration is used.
        let declaration = crate::container::tests::declaration(json!({"oci_ref":"oci:missing-fixture"}));
        let target = CommandConfig::new(&schema, HashMap::new(), &args)
            .unwrap()
            .with_container(Some(declaration));
        let error = try_execute_with_client(
            &target,
            Some(temporary.path()),
            fixture.client(),
            Some("fixture-name"),
            None,
            ClearMode::Never,
            YesFlags::all(),
        )
        .await
        .unwrap_err();
        assert!(format!("{error:#}").contains("untrusted writable ancestor"));
        assert_eq!(std::fs::metadata(&state).unwrap().permissions().mode() & 0o777, 0o777);
        assert_eq!(std::fs::read_dir(&state).unwrap().count(), 0);
        {
            let state = fixture.state.lock().unwrap();
            assert!(state.reservations.is_empty());
            assert!(state.submits.is_empty());
            assert_eq!(state.begin_count, 0);
        }
        fixture.close().await;
    }

    #[tokio::test]
    async fn existing_managed_module_update_keeps_container_and_preflights_without_pro() {
        let fixture = Fixture::new().await;
        let temporary = crate::container::publish::tests::temporary_directory();
        let wasm = temporary.path().join("module.wasm");
        std::fs::write(&wasm, deployment::system_empty::empty().bytes.as_ref()).unwrap();
        let prior_request = fixture.record(false, false).request().unwrap();
        let prior = DeploymentStatus {
            database_identity: database(),
            last_operation: Some(Uuid::from_u128(uuid::Uuid::now_v7().as_u128())),
            revision: Some(prior_request.manifest.current().deployment.revision().unwrap()),
            deployment: prior_request.manifest.current().deployment.clone(),
            module_artifact: ArtifactReference {
                digest: crate::container::publish::tests::empty_module_artifact().digest,
                size_bytes: crate::container::publish::tests::empty_module_artifact().size_bytes,
            },
        };
        {
            let mut state = fixture.state.lock().unwrap();
            state.permission = false;
            state.prior = Some(prior.clone());
        }
        let command = super::super::cli();
        let schema = super::super::build_publish_schema(&command).unwrap();
        let args = command
            .try_get_matches_from([
                "publish",
                "fixture-name",
                "--server",
                fixture.endpoint.as_str(),
                "--bin-path",
                wasm.to_str().unwrap(),
                "--publication-state-dir",
                temporary.path().join("state").to_str().unwrap(),
                "--publication-wait",
                "0",
            ])
            .unwrap();
        let target = CommandConfig::new(&schema, HashMap::new(), &args).unwrap();
        assert!(try_execute_with_client(
            &target,
            Some(temporary.path()),
            fixture.client(),
            Some("fixture-name"),
            None,
            ClearMode::Never,
            YesFlags::all()
        )
        .await
        .unwrap());
        {
            let state = fixture.state.lock().unwrap();
            assert_eq!(state.preflights, 1);
            assert_eq!(state.reservations.len(), 0);
            let request: PublishRequest = serde_json::from_slice(&state.submits[0]).unwrap();
            assert_eq!(request.manifest.current().envelope.expected_revision, prior.revision);
            assert!(matches!(
                request.manifest.current().envelope.container_action,
                ContainerAction::Keep
            ));
            assert!(matches!(
                request.manifest.current().envelope.module_action,
                ModuleAction::Set(_)
            ));
        }
        fixture.close().await;
    }
    #[tokio::test]
    async fn remove_module_requires_authorized_preflight_before_any_upload_or_submission() {
        let fixture = Fixture::new().await;
        let temporary = crate::container::publish::tests::temporary_directory();
        let request = fixture.record(false, false).request().unwrap();
        {
            let mut state = fixture.state.lock().unwrap();
            state.deny_preflight = true;
            state.prior = Some(DeploymentStatus {
                database_identity: database(),
                last_operation: Some(Uuid::from_u128(uuid::Uuid::now_v7().as_u128())),
                revision: Some(request.manifest.current().deployment.revision().unwrap()),
                deployment: request.manifest.current().deployment.clone(),
                module_artifact: ArtifactReference {
                    digest: crate::container::publish::tests::empty_module_artifact().digest,
                    size_bytes: crate::container::publish::tests::empty_module_artifact().size_bytes,
                },
            });
        }
        let command = super::super::cli();
        let schema = super::super::build_publish_schema(&command).unwrap();
        let args = command
            .try_get_matches_from([
                "publish",
                "fixture-name",
                "--server",
                fixture.endpoint.as_str(),
                "--remove-module",
                "--publication-state-dir",
                temporary.path().to_str().unwrap(),
            ])
            .unwrap();
        let target = CommandConfig::new(&schema, HashMap::new(), &args).unwrap();
        assert!(try_execute_with_client(
            &target,
            Some(temporary.path()),
            fixture.client(),
            Some("fixture-name"),
            None,
            ClearMode::Never,
            YesFlags::all()
        )
        .await
        .is_err());
        {
            let state = fixture.state.lock().unwrap();
            assert_eq!(state.preflights, 1);
            assert_eq!(state.begin_count, 0);
            assert!(state.submits.is_empty());
        }
        fixture.close().await;
    }
    #[tokio::test]
    async fn ordinary_module_target_returns_to_legacy_only_when_no_managed_revision_exists() {
        let fixture = Fixture::new().await;
        let command = super::super::cli();
        let schema = super::super::build_publish_schema(&command).unwrap();
        let args = command
            .try_get_matches_from(["publish", "fixture-name", "--server", fixture.endpoint.as_str()])
            .unwrap();
        let target = CommandConfig::new(&schema, HashMap::new(), &args).unwrap();
        assert!(!try_execute_with_client(
            &target,
            None,
            fixture.client(),
            Some("fixture-name"),
            None,
            ClearMode::Never,
            YesFlags::all()
        )
        .await
        .unwrap());
        assert!(fixture.state.lock().unwrap().submits.is_empty());
        fixture.close().await;
    }
    #[tokio::test]
    async fn resume_entrypoint_ignores_changed_project_and_reuses_original_bytes() {
        let fixture = Fixture::new().await;
        fixture.state.lock().unwrap().lose_submit_before_commit = true;
        let temporary = crate::container::publish::tests::temporary_directory();
        let mut record = fixture.record(false, false);
        let mut request = record.request().unwrap();
        request
            .environment
            .insert("ORIGINAL_ENV".into(), "original-secret-value".into());
        record.request_json = serde_json::to_string(&request).unwrap();
        record.request_digest = spacetimedb_oci::sha256(record.request_json.as_bytes());
        let exact = record.request_json.clone();
        let mut journal = Journal::create(
            temporary.path(),
            record,
            None,
            Some(deployment::system_empty::empty().bytes.as_ref()),
        )
        .unwrap();
        assert!(publish::run(
            &fixture.client(),
            &mut journal,
            None,
            Duration::ZERO,
            CancellationToken::new()
        )
        .await
        .is_err());
        let resume = journal.directory().to_owned();
        drop(journal);
        let cli_config = temporary.path().join("isolated-cli.toml");
        std::fs::write(&cli_config, "spacetimedb_token = 'isolated-fixture-credential'\n").unwrap();
        let config = Config::load(spacetimedb_paths::cli::CliTomlPath::from_path_unchecked(cli_config)).unwrap();
        let changed_project = crate::spacetime_config::LoadedConfig {
            config: serde_json::from_value(json!({"database":"different-target", "module_path":"missing-build-source", "server":"must-never-resolve-this-alias", "env":{"ORIGINAL_ENV":"changed-secret-value"}})).unwrap(),
            config_dir: temporary.path().into(), loaded_files: vec![], has_dev_file: false,
        };
        let args = super::super::cli()
            .try_get_matches_from([
                "publish",
                "--resume-publication",
                resume.to_str().unwrap(),
                "--server",
                fixture.endpoint.as_str(),
                "--publication-wait",
                "0",
            ])
            .unwrap();
        super::super::exec_with_options(config, &args, true, Some(&changed_project))
            .await
            .unwrap();
        assert_eq!(
            fixture.state.lock().unwrap().submits,
            [exact.as_bytes(), exact.as_bytes()]
        );
        fixture.close().await;
    }
    #[test]
    fn nested_targets_keep_only_their_own_container_and_resume_rejects_new_inputs() {
        let config: SpacetimeConfig = serde_json::from_value(json!({"database":"parent", "container":{"image":{"oci_ref":"oci:layout"},"resources":{"cpu_millicores":100,"memory_bytes":67108864,"scratch_bytes":1048576,"pids_max":32}}, "children":[{"database":"child"}]})).unwrap();
        let command = super::super::cli();
        let schema = super::super::build_publish_schema(&command).unwrap();
        let args = command.clone().try_get_matches_from(["publish"]).unwrap();
        let targets = super::super::get_filtered_publish_configs(&config, &command, &schema, &args).unwrap();
        assert_eq!(targets.len(), 2);
        assert!(targets[0].container().is_some());
        assert!(targets[1].container().is_none());
        for flags in [
            ["--resume-publication", "operation", "--remove-module"],
            ["--resume-publication", "operation", "--managed"],
        ] {
            assert!(command
                .clone()
                .try_get_matches_from(std::iter::once("publish").chain(flags))
                .is_err());
        }
    }
}
