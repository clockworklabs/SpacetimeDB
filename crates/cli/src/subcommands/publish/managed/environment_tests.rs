use super::*;
use crate::container::publish::tests::{database, publisher, Fixture};
use serde_json::json;
use spacetimedb_lib::environment::{EnvironmentConstraint, EnvironmentDeclaration, EnvironmentSchema};
use std::collections::{BTreeMap, HashMap};

const KEY: &str = "STDB_MANAGED_FIXTURE_REQUIRED";

fn declared_module() -> deployment::system_empty::GeneratedModule {
    deployment::system_empty::generate(
        &EnvironmentSchema::new(vec![EnvironmentDeclaration {
            name: KEY.into(),
            constraint: EnvironmentConstraint::OneOf(vec!["first-secret".into(), "second-secret".into()]),
            optional: false,
        }])
        .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn environment_only_keeps_components_and_retains_exact_mutation_intent() {
    let fixture = Fixture::new().await;
    let temporary = crate::container::publish::tests::temporary_directory();
    let module = declared_module();
    let deployment = deployment::DeploymentSpec::V1(deployment::DeploymentSpecV1 {
        module: deployment::ModuleComponent::SystemEmpty(module.descriptor),
        container: None,
    });
    let prior = DeploymentStatus {
        database_identity: database(),
        revision: Some(deployment.revision().unwrap()),
        deployment,
        last_operation: Some(Uuid::from_u128(uuid::Uuid::now_v7().as_u128())),
        module_artifact: ArtifactReference {
            digest: spacetimedb_oci::sha256(&module.bytes),
            size_bytes: module.bytes.len() as u64,
        },
    };
    fixture.state.lock().unwrap().selected_schema = Some((
        module.descriptor.program_hash,
        crate::container::publish::tests::environment_tests::schema_bytes(
            deployment::system_empty::verify(&module.descriptor, &module.bytes).unwrap(),
        ),
    ));
    let command = super::super::cli();
    let schema = super::super::build_publish_schema(&command).unwrap();
    let args = command
        .try_get_matches_from(["publish", "--env-only", "--unset-env", "UNUSED"])
        .unwrap();
    let target = CommandConfig::new(
        &schema,
        HashMap::from([("env".into(), json!({KEY:"first-secret"}))]),
        &args,
    )
    .unwrap();
    let journal = prepare_request(
        &fixture.client(),
        &target,
        temporary.path(),
        &temporary.path().join("state"),
        Some(&prior),
        None,
        None,
        publisher(),
        &fixture.endpoint,
        YesFlags::all(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let request = journal.record.request().unwrap();
    assert_eq!(
        request.environment,
        BTreeMap::from([(KEY.into(), "first-secret".into())])
    );
    assert_eq!(
        request.manifest.current().envelope.expected_last_operation,
        prior.last_operation
    );
    assert_eq!(request.manifest.current().envelope.expected_revision, prior.revision);
    assert!(matches!(
        request.manifest.current().envelope.module_action,
        ModuleAction::Keep
    ));
    assert!(matches!(
        request.manifest.current().envelope.container_action,
        ContainerAction::Keep
    ));
    assert_eq!(request.environment_remove, ["UNUSED"]);
    assert!(!request.environment_replace);
    assert!(journal.record.uploads.is_empty());
    assert!(!std::fs::read_to_string(journal.directory().join("publication.json"))
        .unwrap()
        .contains("first-secret"));
    let path = journal.directory().to_owned();
    let exact = journal.submission_bytes().unwrap();
    drop(journal);
    // Neither the selected program endpoint nor mutable project input is read
    // again after creation. The exact protected input drives recovery.
    fixture.state.lock().unwrap().selected_schema = None;
    std::fs::write(
        temporary.path().join("spacetime.json"),
        format!(r#"{{"env":{{"{KEY}":"second-secret"}}}}"#),
    )
    .unwrap();
    let reopened = Journal::open(&path).unwrap();
    assert_eq!(reopened.submission_bytes().unwrap(), exact);
    assert_eq!(fixture.state.lock().unwrap().module_gets, 1);
    fixture.close().await;
}

#[tokio::test]
async fn precompiled_inputs_validate_supplied_constraints_without_requiring_complete_values() {
    let fixture = Fixture::new().await;
    let temporary = crate::container::publish::tests::temporary_directory();
    let module = declared_module();
    let wasm = temporary.path().join("exact.wasm");
    std::fs::write(&wasm, &module.bytes).unwrap();
    for (n, values, valid) in [
        (0, json!({}), true),
        (1, json!({KEY:"secret-invalid-value"}), false),
        (2, json!({KEY:"first-secret", "UNKNOWN":"secret-unknown-value"}), true),
        (3, json!({KEY:"first-secret"}), true),
    ] {
        let command = super::super::cli();
        let schema = super::super::build_publish_schema(&command).unwrap();
        let args = command
            .try_get_matches_from(["publish", "--bin-path", wasm.to_str().unwrap()])
            .unwrap();
        let target = CommandConfig::new(&schema, HashMap::from([("env".into(), values)]), &args).unwrap();
        let result = prepare_request(
            &fixture.client(),
            &target,
            temporary.path(),
            &temporary.path().join(format!("state{n}")),
            None,
            None,
            None,
            publisher(),
            &fixture.endpoint,
            YesFlags::all(),
            CancellationToken::new(),
        )
        .await;
        if valid {
            let journal = result.unwrap();
            assert_eq!(
                std::fs::read(journal.directory().join("module.blob")).unwrap(),
                module.bytes.as_ref()
            );
            if n != 0 {
                assert_eq!(journal.record.request().unwrap().environment[KEY], "first-secret");
            }
        } else {
            let error = format!("{:#}", result.err().unwrap());
            assert!(!error.contains("secret-invalid-value") && !error.contains("secret-unknown-value"));
            assert!(!temporary.path().join(format!("state{n}")).exists());
        }
    }
    assert_eq!(fixture.state.lock().unwrap().authenticated, 0);
    fixture.close().await;
}

#[tokio::test]
async fn fresh_process_resume_replays_original_values_after_shell_changes() {
    let fixture = Fixture::new().await;
    fixture.state.lock().unwrap().lose_submit_before_commit = true;
    let temporary = crate::container::publish::tests::temporary_directory();
    let mut record = fixture.record(false, false);
    let mut request = record.request().unwrap();
    request.environment.insert(KEY.into(), "first-secret".into());
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
    let directory = journal.directory().to_owned();
    drop(journal);
    let marker = temporary.path().join("child-finished");
    let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "subcommands::publish::managed::environment_tests::resume_changed_shell_child",
        ])
        .env_clear()
        .env("STDB_MANAGED_FIXTURE_RESUME", &directory)
        .env("STDB_MANAGED_FIXTURE_MARKER", &marker)
        .env(KEY, "second-secret")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(20), child.wait()).await;
    let status = match result {
        Ok(status) => status.unwrap(),
        Err(error) => {
            child.start_kill().unwrap();
            child.wait().await.unwrap();
            panic!("resume child exceeded its deadline: {error}");
        }
    };
    assert!(status.success());
    assert_eq!(std::fs::read(&marker).unwrap(), b"one fixture completed");
    assert_eq!(
        fixture.state.lock().unwrap().submits,
        [exact.as_bytes(), exact.as_bytes()]
    );
    fixture.close().await;
}

#[tokio::test]
#[ignore = "invoked only by the owned fresh-process resume fixture"]
async fn resume_changed_shell_child() {
    assert_eq!(std::env::var(KEY).unwrap(), "second-secret");
    let directory = PathBuf::from(std::env::var_os("STDB_MANAGED_FIXTURE_RESUME").unwrap());
    let marker = PathBuf::from(std::env::var_os("STDB_MANAGED_FIXTURE_MARKER").unwrap());
    assert!(directory.is_absolute() && marker.is_absolute());
    let mut journal = Journal::open(&directory).unwrap();
    assert!(journal.record.server.starts_with("http://127.0.0.1:"));
    let client = PublisherClient::new(
        &journal.record.server,
        "Bearer isolated-fixture-credential".parse().unwrap(),
    )
    .unwrap();
    publish::run(&client, &mut journal, None, Duration::ZERO, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(journal.record.request().unwrap().environment[KEY], "first-secret");
    drop(journal);
    std::fs::write(marker, b"one fixture completed").unwrap();
}
