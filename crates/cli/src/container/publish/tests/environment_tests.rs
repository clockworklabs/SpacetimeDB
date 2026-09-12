use super::*;

fn set_environment(record: &mut Record, values: BTreeMap<String, String>) {
    let mut request = record.request().unwrap();
    request.environment = values;
    record.request_json = serde_json::to_string(&request).unwrap();
    record.request_digest = spacetimedb_oci::sha256(record.request_json.as_bytes());
}

#[tokio::test]
async fn complete_large_environment_is_private_and_exact_after_lost_reply_and_revocation() {
    let fixture = Fixture::new().await;
    fixture.state.lock().unwrap().lose_submit_after_commit = true;
    let temporary = crate::container::publish::tests::temporary_directory();
    let mut record = fixture.record(false, false);
    set_environment(
        &mut record,
        (0..128)
            .map(|n| (format!("KEY_{n}"), "secret-body-sentinel".repeat(256)))
            .collect(),
    );
    assert!(record.request_json.len() > 256 * 1024);
    let exact = record.request_json.clone();
    let mut retained = journal(temporary.path(), record);
    let path = retained.directory().to_owned();
    let metadata = std::fs::read_to_string(path.join("publication.json")).unwrap();
    assert!(!metadata.contains("secret-body-sentinel") && !metadata.contains("request_json"));
    assert_eq!(std::fs::read(path.join("submission.json")).unwrap(), exact.as_bytes());
    assert!(run_now(&fixture.client(), &mut retained).await.is_err());
    drop(retained);
    {
        let mut state = fixture.state.lock().unwrap();
        state.deny_status = true;
        state.permission = false;
        state.deny_upload = true;
    }
    let mut retained = Journal::open(&path).unwrap();
    assert!(matches!(
        run_now(&fixture.client(), &mut retained).await.unwrap(),
        Outcome::Complete(_)
    ));
    assert_eq!(
        fixture.state.lock().unwrap().submits,
        [exact.as_bytes(), exact.as_bytes()]
    );
    drop(retained);
    fixture.close().await;
}

#[tokio::test]
async fn missing_changed_and_malformed_secret_body_never_becomes_empty_or_leaks_in_errors() {
    let fixture = Fixture::new().await;
    for replacement in [
        None,
        Some(b"{}".as_slice()),
        Some(br#"{"environment":{"KEY":"secret-body-sentinel", "KEY":17}}"#.as_slice()),
    ] {
        let temporary = crate::container::publish::tests::temporary_directory();
        let mut record = fixture.record(false, false);
        set_environment(&mut record, BTreeMap::from([("KEY".into(), "original-secret".into())]));
        let retained = journal(temporary.path(), record);
        let path = retained.directory().to_owned();
        if let Some(bytes) = replacement {
            std::fs::write(path.join("submission.json"), bytes).unwrap();
        } else {
            std::fs::remove_file(path.join("submission.json")).unwrap();
        }
        assert!(retained.submission_bytes().is_err());
        drop(retained);
        let error = Journal::open(&path).err().unwrap();
        assert!(!format!("{error:#}").contains("secret-body-sentinel"));
    }
    assert_eq!(fixture.state.lock().unwrap().authenticated, 0);
    fixture.close().await;
}

#[tokio::test]
async fn status_must_match_previous_operation_and_the_first_confirmed_epoch() {
    let fixture = Fixture::new().await;
    let temporary = crate::container::publish::tests::temporary_directory();
    let mut retained = journal(temporary.path(), fixture.record(false, false));
    run_now(&fixture.client(), &mut retained).await.unwrap();
    let request = retained.record.request().unwrap();
    for which in 0..3 {
        let mut status = retained.record.status.clone().unwrap();
        match which {
            0 => status.expected_last_operation = Some(Uuid::from_u128(uuid::Uuid::now_v7().as_u128())),
            1 => status.publication_epoch = 0,
            _ => status.publication_epoch += 1,
        }
        assert!(retained.record.check_status(&request, &status).is_err());
    }
    fixture.close().await;
}

#[cfg(unix)]
#[tokio::test]
async fn protected_storage_rejects_public_modes_symlinks_and_hardlinks() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let fixture = Fixture::new().await;
    let temporary = crate::container::publish::tests::temporary_directory();
    let base = temporary.path().join("new/retained");
    let retained = journal(&base, fixture.record(false, false));
    for parent in [temporary.path().join("new"), base] {
        assert_eq!(std::fs::metadata(parent).unwrap().permissions().mode() & 0o777, 0o700);
    }
    let path = retained.directory().to_owned();
    let body = path.join("submission.json");
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o700);
    for name in ["submission.json", "publication.json", "publication.lock"] {
        assert_eq!(
            std::fs::metadata(path.join(name)).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    std::fs::set_permissions(&body, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(retained.submission_bytes().is_err());
    std::fs::set_permissions(&body, std::fs::Permissions::from_mode(0o600)).unwrap();
    let link = temporary.path().join("extra-link");
    std::fs::hard_link(&body, &link).unwrap();
    assert!(retained.submission_bytes().is_err());
    std::fs::remove_file(&link).unwrap();
    std::fs::rename(&body, &link).unwrap();
    symlink(&link, &body).unwrap();
    assert!(retained.submission_bytes().is_err());
    drop(retained);
    assert!(Journal::open(&path).is_err());
    std::fs::remove_file(&body).unwrap();
    std::fs::rename(&link, &body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Journal::open(&path).is_err());
    fixture.close().await;
}

pub(crate) fn schema_bytes(environment: spacetimedb_lib::environment::EnvironmentSchema) -> Vec<u8> {
    use spacetimedb_lib::db::raw_def::v10::{RawModuleDefV10, RawModuleDefV10Section};
    serde_json::to_vec(&spacetimedb_lib::sats::serde::SerdeWrapper(RawModuleDefV10 {
        sections: vec![
            RawModuleDefV10Section::Typespace(Default::default()),
            RawModuleDefV10Section::Environment(environment.into_declarations()),
        ],
    }))
    .unwrap()
}

#[tokio::test]
async fn kept_schema_checks_identity_program_hash_and_bounded_v10_metadata() {
    let fixture = Fixture::new().await;
    let request = fixture.record(false, false).request().unwrap();
    let prior = DeploymentStatus {
        database_identity: database(),
        revision: Some(request.manifest.current().deployment.revision().unwrap()),
        last_operation: Some(request.manifest.current().envelope.operation_id),
        deployment: request.manifest.current().deployment.clone(),
        module_artifact: ArtifactReference {
            digest: empty_module_artifact().digest,
            size_bytes: empty_module_artifact().size_bytes,
        },
    };
    let hash = system_empty::empty().descriptor.program_hash;
    fixture.state.lock().unwrap().selected_schema = Some((hash, schema_bytes(Default::default())));
    assert_eq!(
        fixture.client().selected_environment(&prior).await.unwrap(),
        Default::default()
    );
    fixture.state.lock().unwrap().wrong_module_identity = true;
    assert!(fixture.client().selected_environment(&prior).await.is_err());
    fixture.state.lock().unwrap().wrong_module_identity = false;
    for (hash, bytes) in [
        (spacetimedb_lib::Hash::ZERO, schema_bytes(Default::default())),
        (hash, b"invalid-private-schema-sentinel".to_vec()),
        (hash, vec![0; 16 * 1024 * 1024 + 1]),
    ] {
        fixture.state.lock().unwrap().selected_schema = Some((hash, bytes));
        let error = fixture.client().selected_environment(&prior).await.unwrap_err();
        assert!(!format!("{error:#}").contains("invalid-private-schema-sentinel"));
    }
    fixture.close().await;
}

#[tokio::test]
async fn malformed_server_response_cannot_disclose_echoed_environment_values() {
    let fixture = Fixture::new().await;
    fixture.state.lock().unwrap().malformed_status = true;
    let temporary = crate::container::publish::tests::temporary_directory();
    let mut retained = journal(temporary.path(), fixture.record(false, false));
    let error = run_now(&fixture.client(), &mut retained).await.unwrap_err();
    assert!(!format!("{error:#}").contains("secret-body-sentinel"));
    assert!(retained.record.submitted && retained.record.status.is_none());
    fixture.close().await;
}

#[cfg(unix)]
#[tokio::test]
async fn untrusted_writable_parent_is_rejected_before_retaining_secrets() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new().await;
    let temporary = crate::container::publish::tests::temporary_directory();
    let base = temporary.path().join("untrusted");
    std::fs::create_dir(&base).unwrap();
    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o777)).unwrap();
    let record = fixture.record(false, false);
    assert!(Journal::create(&base, record, None, Some(system_empty::empty().bytes.as_ref())).is_err());
    assert_eq!(std::fs::read_dir(&base).unwrap().count(), 0);
    assert_eq!(fixture.state.lock().unwrap().authenticated, 0);
    fixture.close().await;
}
