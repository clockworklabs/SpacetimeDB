//! Actual module persistence with explicit local input, without a server or CLI configuration.
use futures::FutureExt as _;
use spacetimedb::config::{CertificateAuthority, ModuleHttpConfig, V8Config, WasmConfig};
use spacetimedb::db;
use spacetimedb::db::persistence::DurabilityConfig;
use spacetimedb::host::{FunctionArgs, UpdateDatabaseResult};
use spacetimedb::messages::control_db::HostType;
use spacetimedb::util::jobs::JobCores;
use spacetimedb_client_api::routes::subscribe::WebSocketOptions;
use spacetimedb_client_api::{
    ControlStateReadAccess as _, ControlStateWriteAccess as _, DatabaseDef, DatabaseResetDef, NodeDelegate as _,
};
use spacetimedb_lib::{bsatn, sats::product, AlgebraicValue, Identity};
use spacetimedb_paths::cli::{PrivKeyPath, PubKeyPath};
use spacetimedb_paths::{server::ServerDataDir, FromPathUnchecked};
use spacetimedb_schema::auto_migrate::MigrationPolicy;
use spacetimedb_standalone::{StandaloneEnv, StandaloneOptions};
use spacetimedb_testing::modules::{start_runtime, CompilationMode, CompiledModule};
use std::{collections::BTreeMap, sync::Arc};

type Values = BTreeMap<String, String>;

async fn read(env: &StandaloneEnv, database: u64, key: &str) -> anyhow::Result<AlgebraicValue> {
    let module = env.leader(database).await?.module().await?;
    Ok(module
        .call_procedure(
            Identity::ZERO,
            None,
            None,
            "read_environment",
            FunctionArgs::Bsatn(bsatn::to_vec(&product![key])?.into()),
        )
        .await
        .result?
        .return_val)
}

#[test]
#[serial_test::serial]
fn real_module_reopen_and_environment_only_publication_preserve_values() -> anyhow::Result<()> {
    let compiled = CompiledModule::compile("environment-test", CompilationMode::Debug);
    let bytes = compiled.program_bytes();
    start_runtime().block_on(async move {
        let temp = tempfile::tempdir()?;
        let keys = temp.path().join("keys");
        std::fs::create_dir(&keys)?;
        let ca = CertificateAuthority {
            jwt_pub_key_path: PubKeyPath(keys.join("public")),
            jwt_priv_key_path: PrivKeyPath(keys.join("private")),
        };
        let data_dir = Arc::new(ServerDataDir::from_path_unchecked(temp.path().join("data")));
        data_dir.create()?;
        let config = StandaloneOptions {
            db_config: db::Config {
                storage: db::Storage::Disk,
                page_pool_max_size: None,
            },
            durability: DurabilityConfig::default(),
            websocket: WebSocketOptions::default(),
            module_http: ModuleHttpConfig::default(),
            wasm: WasmConfig::default(),
            v8: V8Config::default(),
        };
        let mut env = Some(StandaloneEnv::init(config, &ca, data_dir.clone(), JobCores::without_pinned_cores()).await?);
        let run = std::panic::AssertUnwindSafe(async {
            let initial = Values::from([
                ("REQUIRED".into(), "initial-required".into()),
                ("MODE".into(), "ready".into()),
            ]);
            let spec = |environment| DatabaseDef {
                database_identity: Identity::ZERO,
                program_bytes: bytes.clone(),
                environment,
                environment_remove: Vec::new(),
                environment_replace: false,
                expected_module_version: None,
                num_replicas: None,
                host_type: HostType::Wasm,
                parent: None,
                organization: None,
            };
            log::info!("ENV standalone fixture: initial publication");
            assert!(env
                .as_ref()
                .unwrap()
                .publish_database(&Identity::ZERO, spec(initial.clone()), MigrationPolicy::Compatible)
                .await?
                .is_none());
            let database = env
                .as_ref()
                .unwrap()
                .get_database_by_identity(&Identity::ZERO)
                .await?
                .unwrap();
            let replica = env
                .as_ref()
                .unwrap()
                .get_leader_replica_by_database(database.id)
                .await
                .unwrap();
            log::info!("ENV standalone fixture: rejected publication preserves live host");
            for changed_program in [false, true] {
                let mut invalid = spec(Values::new());
                invalid.environment_replace = true;
                if changed_program {
                    // An empty custom section changes the Wasm hash without changing
                    // its declarations, exercising rejection of a candidate module.
                    let mut bytes = invalid.program_bytes.to_vec();
                    bytes.extend_from_slice(&[0, 1, 0]);
                    invalid.program_bytes = bytes.into();
                }
                let rejected = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    env.as_ref()
                        .unwrap()
                        .publish_database(&Identity::ZERO, invalid, MigrationPolicy::Compatible),
                )
                .await
                .expect("rejected publication must not hang");
                assert!(rejected.is_err(), "missing required values must reject publication");
                assert_eq!(
                    read(env.as_ref().unwrap(), database.id, "REQUIRED").await?,
                    AlgebraicValue::from(Some("initial-required".to_owned()))
                );
            }
            // An omitted input preserves required values. Environment-only requests
            // also retain the module instance and cannot invoke init again.
            assert!(matches!(
                env.as_ref()
                    .unwrap()
                    .publish_database(&Identity::ZERO, spec(Values::new()), MigrationPolicy::Compatible)
                    .await?,
                Some(UpdateDatabaseResult::NoUpdateNeeded)
            ));
            let previous_module = env.as_ref().unwrap().leader(database.id).await?.module().await?;
            let version = previous_module.info.module_hash;
            let mut env_only = spec(Values::from([("FUTURE".into(), "undeclared".into())]));
            env_only.program_bytes = Default::default();
            env_only.expected_module_version = Some(version);
            assert!(env
                .as_ref()
                .unwrap()
                .publish_database(&Identity::ZERO, env_only, MigrationPolicy::Compatible)
                .await?
                .unwrap()
                .was_successful());
            let current = env.as_ref().unwrap().leader(database.id).await?.module().await?;
            assert!(Arc::ptr_eq(&previous_module.info, &current.info));
            let metadata = env
                .as_ref()
                .unwrap()
                .leader(database.id)
                .await?
                .environment_metadata()
                .await?;
            assert!(metadata.stored_keys.iter().any(|key| key == "FUTURE"));
            assert_eq!(
                read(env.as_ref().unwrap(), database.id, "REQUIRED").await?,
                AlgebraicValue::from(Some("initial-required".to_owned()))
            );
            assert!(read(env.as_ref().unwrap(), database.id, "FUTURE").await.is_err());
            let mut stale = spec(Values::new());
            stale.program_bytes = Default::default();
            stale.expected_module_version = Some(spacetimedb_lib::Hash::from_hex("00".repeat(32))?);
            assert!(env
                .as_ref()
                .unwrap()
                .publish_database(&Identity::ZERO, stale, MigrationPolicy::Compatible)
                .await
                .is_err());
            log::info!("ENV standalone fixture: same-program update");
            let mut updated = initial.clone();
            updated.insert("REQUIRED".into(), "republished".into());
            assert!(env
                .as_ref()
                .unwrap()
                .publish_database(&Identity::ZERO, spec(updated), MigrationPolicy::Compatible)
                .await?
                .unwrap()
                .was_successful());
            assert_eq!(
                read(env.as_ref().unwrap(), database.id, "REQUIRED").await?,
                AlgebraicValue::from(Some("republished".to_owned()))
            );
            // Close the server's module and storage, then reopen the same data directory.
            // Init requires initial-required, so reading republished proves it was not run again.
            drop(previous_module);
            drop(current);
            let module = env.as_ref().unwrap().leader(database.id).await?.module().await?;
            module.exit().await;
            module.relational_db().shutdown().await;
            drop(module);
            drop(env.take());
            env = Some(StandaloneEnv::init(config, &ca, data_dir.clone(), JobCores::without_pinned_cores()).await?);
            // The first request after restart is env-only, before leader lookup has started a host.
            let mut cold = spec(Values::from([("EMPTY".into(), "after-restart".into())]));
            cold.program_bytes = Default::default();
            cold.expected_module_version = Some(version);
            assert!(env
                .as_ref()
                .unwrap()
                .publish_database(&Identity::ZERO, cold, MigrationPolicy::Compatible)
                .await?
                .unwrap()
                .was_successful());
            assert_eq!(
                read(env.as_ref().unwrap(), database.id, "EMPTY").await?,
                AlgebraicValue::from(Some("after-restart".to_owned()))
            );
            assert_eq!(
                read(env.as_ref().unwrap(), database.id, "REQUIRED").await?,
                AlgebraicValue::from(Some("republished".to_owned()))
            );
            // Init asserts initial-required. A successful reopen with republished proves
            // that init was not run again and the initial environment values were not restored.
            assert!(env
                .as_ref()
                .unwrap()
                .reset_database(
                    &Identity::ZERO,
                    DatabaseResetDef {
                        database_identity: Identity::ZERO,
                        program_bytes: None,
                        environment_remove: Default::default(),
                        environment_replace: false,
                        environment: Values::new(),
                        num_replicas: None,
                        host_type: None,
                    }
                )
                .await
                .is_err());
            assert_eq!(
                read(env.as_ref().unwrap(), database.id, "REQUIRED").await?,
                AlgebraicValue::from(Some("republished".to_owned()))
            );
            log::info!("ENV standalone fixture: complete no-artifact reset");
            let mut reset = initial;
            reset.insert("EMPTY".into(), "reset-input".into());
            env.as_ref()
                .unwrap()
                .reset_database(
                    &Identity::ZERO,
                    DatabaseResetDef {
                        database_identity: Identity::ZERO,
                        program_bytes: None,
                        environment_remove: Default::default(),
                        environment_replace: false,
                        environment: reset,
                        num_replicas: None,
                        host_type: None,
                    },
                )
                .await?;
            assert_ne!(
                env.as_ref()
                    .unwrap()
                    .get_leader_replica_by_database(database.id)
                    .await
                    .unwrap()
                    .id,
                replica.id
            );
            assert_eq!(
                read(env.as_ref().unwrap(), database.id, "EMPTY").await?,
                AlgebraicValue::from(Some("reset-input".to_owned()))
            );
            anyhow::Ok(())
        })
        .catch_unwind()
        .await;
        // Join physical cleanup even if an assertion in the accepted fixture task
        // panicked. Never let a failed test detach its database host.
        log::info!("ENV standalone fixture: positive cleanup");
        if let Some(env) = &env {
            env.delete_database(&Identity::ZERO, &Identity::ZERO).await?;
        }
        match run {
            Ok(result) => result?,
            Err(panic) => std::panic::resume_unwind(panic),
        }
        assert!(env
            .as_ref()
            .unwrap()
            .get_database_by_identity(&Identity::ZERO)
            .await?
            .is_none());
        Ok(())
    })
}
