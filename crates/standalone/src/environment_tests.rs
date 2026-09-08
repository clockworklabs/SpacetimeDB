//! Actual module persistence with explicit local input, without a server or CLI configuration.
use super::*;
use spacetimedb::host::FunctionArgs;
use spacetimedb_client_api::{ControlStateWriteAccess as _, DatabaseDef};
use spacetimedb_lib::{bsatn, sats::product, AlgebraicValue};
use spacetimedb_paths::cli::{PrivKeyPath, PubKeyPath};
use spacetimedb_paths::FromPathUnchecked;
use std::collections::BTreeMap;

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

#[tokio::test]
#[ignore = "requires an explicitly configured local environment-test Wasm artifact"]
async fn real_module_reopen_and_no_artifact_reset_preserve_complete_environment_semantics() -> anyhow::Result<()> {
    let module_path = std::path::PathBuf::from(
        std::env::var_os("SPACETIMEDB_ENV_STANDALONE_TEST_MODULE")
            .context("SPACETIMEDB_ENV_STANDALONE_TEST_MODULE must name the owned local fixture")?,
    );
    anyhow::ensure!(module_path.is_absolute(), "fixture path must be absolute");
    let bytes = axum::body::Bytes::from(std::fs::read(module_path)?);
    anyhow::ensure!(bytes.starts_with(b"\0asm"), "fixture must be a Wasm module");
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
    let env = StandaloneEnv::init(config, &ca, data_dir, JobCores::without_pinned_cores()).await?;
    let test_env = env.clone();
    let run = tokio::spawn(async move {
        let env = test_env;
        let initial = Values::from([
            ("REQUIRED".into(), "initial-required".into()),
            ("MODE".into(), "ready".into()),
        ]);
        let spec = |environment| DatabaseDef {
            database_identity: Identity::ZERO,
            program_bytes: bytes.clone(),
            environment,
            num_replicas: None,
            host_type: HostType::Wasm,
            parent: None,
            organization: None,
        };
        log::info!("ENV standalone fixture: initial publication");
        assert!(env
            .publish_database(&Identity::ZERO, spec(initial.clone()), MigrationPolicy::Compatible)
            .await?
            .is_none());
        let database = env.control_db.get_database_by_identity(&Identity::ZERO)?.unwrap();
        let replica = env.control_db.get_leader_replica_by_database(database.id).unwrap();
        log::info!("ENV standalone fixture: rejected publication preserves live host");
        let rejected = env
            .publish_database(&Identity::ZERO, spec(Values::new()), MigrationPolicy::Compatible)
            .await;
        assert!(
            rejected.as_ref().is_err()
                || rejected
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .is_some_and(|result| !result.was_successful())
        );
        assert_eq!(
            read(&env, database.id, "REQUIRED").await?,
            AlgebraicValue::from(Some("initial-required".to_owned()))
        );
        log::info!("ENV standalone fixture: same-program update");
        let mut updated = initial.clone();
        updated.insert("REQUIRED".into(), "republished".into());
        assert!(env
            .publish_database(&Identity::ZERO, spec(updated), MigrationPolicy::Compatible)
            .await?
            .unwrap()
            .was_successful());
        assert_eq!(
            read(&env, database.id, "REQUIRED").await?,
            AlgebraicValue::from(Some("republished".to_owned()))
        );
        // The old private bootstrap input intentionally remains distinct. Ordinary
        // reopen must consult persisted st_module/st_env and skip that input.
        assert_eq!(
            env.control_db.initial_environment(&database)?["REQUIRED"],
            "initial-required"
        );
        log::info!("ENV standalone fixture: positive close and normal reopen");
        let replica_id = replica.id;
        env.own_publication(
            move |owner| async move { owner.host_controller.exit_module_host_and_join(replica_id).await },
        )
        .await?;
        assert_eq!(
            read(&env, database.id, "REQUIRED").await?,
            AlgebraicValue::from(Some("republished".to_owned()))
        );
        // Init asserts initial-required. A successful reopen with republished proves
        // that init was not run again and old bootstrap input was not restored.
        assert!(env
            .reset_database(
                &Identity::ZERO,
                DatabaseResetDef {
                    database_identity: Identity::ZERO,
                    program_bytes: None,
                    environment: Values::new(),
                    num_replicas: None,
                    host_type: None,
                }
            )
            .await
            .is_err());
        assert_eq!(
            env.control_db
                .get_database_by_id(database.id)?
                .unwrap()
                .bootstrap_generation,
            1
        );
        assert_eq!(
            read(&env, database.id, "REQUIRED").await?,
            AlgebraicValue::from(Some("republished".to_owned()))
        );
        log::info!("ENV standalone fixture: complete no-artifact reset");
        let mut reset = initial;
        reset.insert("EMPTY".into(), "reset-input".into());
        env.reset_database(
            &Identity::ZERO,
            DatabaseResetDef {
                database_identity: Identity::ZERO,
                program_bytes: None,
                environment: reset,
                num_replicas: None,
                host_type: None,
            },
        )
        .await?;
        assert_eq!(
            env.control_db
                .get_database_by_id(database.id)?
                .unwrap()
                .bootstrap_generation,
            2
        );
        assert_ne!(
            env.control_db.get_leader_replica_by_database(database.id).unwrap().id,
            replica.id
        );
        assert_eq!(
            read(&env, database.id, "EMPTY").await?,
            AlgebraicValue::from(Some("reset-input".to_owned()))
        );
        anyhow::Ok(())
    })
    .await;
    // Join physical cleanup even if an assertion in the accepted fixture task
    // panicked. Never let a failed test detach its database host.
    log::info!("ENV standalone fixture: positive cleanup");
    env.delete_database(&Identity::ZERO, &Identity::ZERO).await?;
    run??;
    assert!(env.control_db.get_database_by_identity(&Identity::ZERO)?.is_none());
    Ok(())
}
