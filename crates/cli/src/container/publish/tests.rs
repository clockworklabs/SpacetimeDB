//! These fixtures own numeric-loopback sockets and in-memory credentials. They
//! never load CLI configuration, environment endpoints, or external builders.
use super::*;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    Json, Router,
};
use client::{ObjectRef, UploadKind, UploadStatus};
use journal::{Record, UploadRecord};
use serde_json::json;
use spacetimedb_lib::{
    deployment::{api::*, manifest::*, *},
    Identity, Uuid,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

const TOKEN: &str = "Bearer isolated-fixture-credential";
pub(crate) fn empty_module_artifact() -> ModuleArtifact {
    let bytes = system_empty::empty().bytes.as_ref();
    ModuleArtifact {
        digest: spacetimedb_oci::sha256(bytes),
        size_bytes: bytes.len() as u64,
    }
}

pub(crate) fn publisher() -> Identity {
    Identity::from_be_byte_array([42; 32])
}
pub(crate) fn database() -> Identity {
    Identity::from_be_byte_array([43; 32])
}
#[derive(Default)]
pub(crate) struct Behavior {
    pub permission: bool,
    pub lose_append: bool,
    pub lose_completion: bool,
    pub lose_submit_after_commit: bool,
    pub lose_submit_before_commit: bool,
    pub lose_reservation: bool,
    pub stale: bool,
    pub bad_receipt: bool,
    pub bad_completion: bool,
    pub bad_status: bool,
    pub wrong_reservation: bool,
    pub deny_preflight: bool,
    pub deny_upload: bool,
    pub deny_status: bool,
    pub fail_naming: bool,
    pub redirect: Option<String>,
    pub prior: Option<DeploymentStatus>,
    pub uploads: BTreeMap<uuid::Uuid, (UploadStatus, Vec<u8>)>,
    pub submits: Vec<Vec<u8>>,
    pub reservations: Vec<ReserveDatabaseRequest>,
    pub names: usize,
    pub preflights: usize,
    pub begin_count: usize,
    pub status: Option<PublicationStatus>,
    pub authenticated: usize,
}
pub(crate) struct Fixture {
    pub endpoint: String,
    pub state: Arc<Mutex<Behavior>>,
    stop: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}
impl Fixture {
    pub async fn new() -> Self {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let endpoint = format!("http://{}/", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(Behavior {
            permission: true,
            ..Default::default()
        }));
        let stop = CancellationToken::new();
        let cancellation = stop.clone();
        let app = Router::new()
            .fallback(handler)
            .layer(axum::extract::DefaultBodyLimit::max(32 * 1024 * 1024))
            .with_state((state.clone(), endpoint.clone()));
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(cancellation.cancelled_owned())
                .await
                .unwrap();
        });
        Self {
            endpoint,
            state,
            stop,
            task,
        }
    }
    pub fn client(&self) -> PublisherClient {
        PublisherClient::new(&self.endpoint, TOKEN.parse().unwrap()).unwrap()
    }
    pub fn record(&self, new: bool, name: bool) -> Record {
        let bytes = system_empty::empty().bytes.as_ref();
        let envelope = PublishEnvelope {
            version: PUBLISH_PROTOCOL_VERSION,
            operation_id: Uuid::from_u128(uuid::Uuid::now_v7().as_u128()),
            expected_revision: None,
            module_action: ModuleAction::Set(UserModule {
                kind: UserModuleKind::Wasm,
                program_hash: spacetimedb_lib::hash_bytes(bytes),
            }),
            container_action: Default::default(),
        };
        let module_artifact = empty_module_artifact();
        let request = PublishRequest {
            manifest: PreparedDeploymentManifest::V1(PreparedDeploymentManifestV1 {
                deployment: envelope.resolve(None, &Default::default()).unwrap(),
                envelope,
                module_artifact,
                migration_policy: PreparedMigrationPolicy::Compatible,
            }),
            creation: new.then_some(CreationOptions {
                parent: None,
                organization: None,
                num_replicas: None,
                enforce_anti_affinity: true,
            }),
            image_source: None,
        };
        let request_json = serde_json::to_string(&request).unwrap();
        Record {
            version: 1,
            server: self.endpoint.clone(),
            artifact_endpoint: self.endpoint.clone(),
            publisher: publisher(),
            database: (!new).then_some(database()),
            reservation: request.creation.clone().map(|options| ReserveDatabaseRequest {
                version: PUBLISH_PROTOCOL_VERSION,
                operation_id: request.manifest.current().envelope.operation_id,
                options,
            }),
            requested_name: name.then_some("fixture-name".into()),
            request_digest: spacetimedb_oci::sha256(request_json.as_bytes()),
            request_json,
            uploads: vec![UploadRecord {
                kind: UploadKind::Module,
                object: ObjectRef {
                    digest: module_artifact.digest,
                    size: module_artifact.size_bytes,
                },
                session: None,
            }],
            submitted: false,
            status: None,
            name_confirmed: false,
            naming_attempted: false,
        }
    }
    pub async fn close(self) {
        self.stop.cancel();
        self.task.await.unwrap();
    }
}
async fn handler(
    State((shared, endpoint)): State<(Arc<Mutex<Behavior>>, String)>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let mut state = shared.lock().unwrap();
    let path = uri.path();
    if path == "/v1/containers/capabilities" {
        assert!(headers.get("authorization").is_none());
        if let Some(location) = &state.redirect {
            return (StatusCode::TEMPORARY_REDIRECT, [("location", location.clone())]).into_response();
        }
        return Json(PublicationCapabilities {
            version: 1,
            enabled: true,
            artifact_endpoint: Some(endpoint),
        })
        .into_response();
    }
    if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some(TOKEN) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    state.authenticated += 1;
    if path == "/v1/containers/publish-permission" {
        return Json(PublishPermission {
            identity: publisher(),
            can_publish: state.permission,
            source_revision: None,
        })
        .into_response();
    }
    if path == "/v1/containers/reservations" {
        let request: ReserveDatabaseRequest = serde_json::from_slice(&body).unwrap();
        if let Some(prior) = state.reservations.first() {
            assert_eq!(prior, &request);
        }
        state.reservations.push(request.clone());
        if std::mem::take(&mut state.lose_reservation) {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        return Json(DatabaseReservation {
            database_identity: database(),
            operation_id: if state.wrong_reservation {
                Uuid::from_u128(uuid::Uuid::now_v7().as_u128())
            } else {
                request.operation_id
            },
            expires_at: "fixture-only".into(),
            staging_open: true,
            artifact_endpoint: endpoint,
        })
        .into_response();
    }
    if path.ends_with("/pre_publish") {
        state.preflights += 1;
        if state.deny_preflight {
            return StatusCode::FORBIDDEN.into_response();
        }
        assert_eq!(body.as_ref(), system_empty::empty().bytes.as_ref());
        return Json(spacetimedb_client_api_messages::name::PrePublishResult::AutoMigrate(
            spacetimedb_client_api_messages::name::PrePublishAutoMigrateResult {
                migrate_plan: "fixture migration".into(),
                break_clients: false,
                token: spacetimedb_lib::Hash::ZERO,
                major_version_upgrade: false,
            },
        ))
        .into_response();
    }
    if path.ends_with("/names") {
        state.names += 1;
        if state.fail_naming {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        return Json(spacetimedb_client_api_messages::name::SetDomainsResult::Success).into_response();
    }
    if path.contains("/deployment/operations/") {
        if state.deny_status {
            return StatusCode::FORBIDDEN.into_response();
        }
        return state
            .status
            .clone()
            .map(|status| Json(status).into_response())
            .unwrap_or_else(|| StatusCode::NOT_FOUND.into_response());
    }
    if path.ends_with("/deployment") {
        if method == Method::GET {
            return state
                .prior
                .clone()
                .map(|p| Json(p).into_response())
                .unwrap_or_else(|| StatusCode::NOT_FOUND.into_response());
        }
        assert_eq!(method, Method::PUT);
        state.submits.push(body.to_vec());
        if let Some(status) = &state.status {
            assert_eq!(state.submits.first().unwrap().as_slice(), body.as_ref());
            return Json(status.clone()).into_response();
        }
        if state.stale {
            return (StatusCode::CONFLICT, "private-server-error-must-not-be-printed").into_response();
        }
        if std::mem::take(&mut state.lose_submit_before_commit) {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        let request: PublishRequest = serde_json::from_slice(&body).unwrap();
        request.manifest.validate(&Default::default()).unwrap();
        let status = PublicationStatus {
            database_identity: database(),
            operation_id: request.manifest.current().envelope.operation_id,
            phase: PublicationPhase::Complete,
            expected_revision: request.manifest.current().envelope.expected_revision,
            proposed_revision: if state.bad_status {
                spacetimedb_lib::Hash::ZERO
            } else {
                request.manifest.current().deployment.revision().unwrap()
            },
            error: None,
        };
        state.status = Some(status.clone());
        if std::mem::take(&mut state.lose_submit_after_commit) {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        return Json(status).into_response();
    }
    if path.ends_with("/uploads") {
        state.begin_count += 1;
        if state.deny_upload {
            return StatusCode::FORBIDDEN.into_response();
        }
        let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            headers["x-spacetimedb-artifact-kind"],
            request["kind"].as_str().unwrap()
        );
        let object: ObjectRef = serde_json::from_value(request["object"].clone()).unwrap();
        let status = UploadStatus {
            id: uuid::Uuid::new_v4(),
            object,
            offset: 0,
            expires_at: u64::MAX,
            complete: false,
        };
        state.uploads.insert(status.id, (status.clone(), vec![]));
        return Json(status).into_response();
    }
    if let Some((_, tail)) = path.split_once("/uploads/") {
        let id: uuid::Uuid = tail.split('/').next().unwrap().parse().unwrap();
        let Some((status, bytes)) = state.uploads.get_mut(&id) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        if method == Method::PATCH {
            assert!(body.len() <= client::UPLOAD_CHUNK_BYTES);
            let offset: u64 = uri.query().unwrap().strip_prefix("offset=").unwrap().parse().unwrap();
            assert_eq!(offset, status.offset);
            bytes.extend_from_slice(&body);
            status.offset += body.len() as u64;
        } else if path.ends_with("/complete") {
            assert_eq!(bytes.len() as u64, status.object.size);
            assert_eq!(spacetimedb_oci::sha256(bytes), status.object.digest);
            status.complete = true;
        }
        let mut response = status.clone();
        if path.ends_with("/complete") {
            if std::mem::take(&mut state.lose_completion) {
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
            let mut object = response.object;
            if state.bad_completion {
                object.size += 1;
            }
            return Json(object).into_response();
        }
        if method == Method::PATCH && std::mem::take(&mut state.lose_append) {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        if state.bad_receipt {
            response.object.size += 1;
        }
        return Json(response).into_response();
    }
    panic!("unexpected fixture request: {method} {uri}");
}
async fn run_now(client: &PublisherClient, journal: &mut Journal) -> Result<Outcome> {
    run(client, journal, None, Duration::ZERO, CancellationToken::new()).await
}
fn journal(base: &Path, record: Record) -> Journal {
    Journal::create(base, record, None, Some(system_empty::empty().bytes.as_ref())).unwrap()
}
use std::path::Path;

#[tokio::test]
async fn lost_append_and_committed_response_resume_without_reupload_or_current_permission() {
    let fixture = Fixture::new().await;
    {
        let mut s = fixture.state.lock().unwrap();
        s.lose_append = true;
        s.lose_submit_after_commit = true;
    }
    let temporary = tempfile::tempdir().unwrap();
    let mut journal = journal(temporary.path(), fixture.record(false, false));
    let exact = journal.record.request_json.clone();
    assert!(run_now(&fixture.client(), &mut journal).await.is_err());
    assert!(journal.record.submitted);
    let path = journal.directory().to_owned();
    drop(journal);
    std::fs::remove_file(path.join("module.blob")).unwrap();
    fixture.state.lock().unwrap().permission = false;
    let mut journal = Journal::open(&path).unwrap();
    assert!(matches!(
        run_now(&fixture.client(), &mut journal).await.unwrap(),
        Outcome::Complete(_)
    ));
    {
        let state = fixture.state.lock().unwrap();
        assert_eq!(state.submits, [exact.into_bytes()]);
        assert_eq!(state.begin_count, 1);
    }
    drop(journal);
    fixture.close().await;
}
#[tokio::test]
async fn lost_reservation_and_unadmitted_put_replay_exact_request_and_generated_identity() {
    let fixture = Fixture::new().await;
    {
        let mut s = fixture.state.lock().unwrap();
        s.lose_reservation = true;
        s.lose_submit_before_commit = true;
    }
    let temporary = tempfile::tempdir().unwrap();
    let mut journal = journal(temporary.path(), fixture.record(true, false));
    let exact = journal.record.request_json.clone();
    assert!(run_now(&fixture.client(), &mut journal).await.is_err());
    assert!(journal.record.database.is_none());
    assert!(run_now(&fixture.client(), &mut journal).await.is_err());
    assert_eq!(journal.record.database, Some(database()));
    assert!(matches!(
        run_now(&fixture.client(), &mut journal).await.unwrap(),
        Outcome::Complete(_)
    ));
    {
        let state = fixture.state.lock().unwrap();
        assert_eq!(state.reservations.len(), 2);
        assert_eq!(state.submits, [exact.as_bytes(), exact.as_bytes()]);
        assert_eq!(state.begin_count, 1);
    }
    fixture.close().await;
}
#[tokio::test]
async fn stale_revision_preserves_operation_and_redacts_response_without_fallback() {
    let fixture = Fixture::new().await;
    fixture.state.lock().unwrap().stale = true;
    let temporary = tempfile::tempdir().unwrap();
    let mut journal = journal(temporary.path(), fixture.record(false, false));
    let exact = journal.record.request_json.clone();
    let error = run_now(&fixture.client(), &mut journal).await.unwrap_err();
    assert!(!format!("{error:#}").contains("private-server-error"));
    assert_eq!(journal.record.request_json, exact);
    assert!(journal.record.submitted);
    assert_eq!(fixture.state.lock().unwrap().submits.len(), 1);
    fixture.close().await;
}
#[tokio::test]
async fn mismatched_reservation_or_upload_receipt_never_reaches_admission() {
    for reservation in [true, false] {
        let fixture = Fixture::new().await;
        {
            let mut s = fixture.state.lock().unwrap();
            s.wrong_reservation = reservation;
            s.bad_receipt = !reservation;
        }
        let temporary = tempfile::tempdir().unwrap();
        let mut journal = journal(temporary.path(), fixture.record(reservation, false));
        assert!(run_now(&fixture.client(), &mut journal).await.is_err());
        assert!(!journal.record.submitted);
        assert!(fixture.state.lock().unwrap().submits.is_empty());
        fixture.close().await;
    }
}
#[tokio::test]
async fn completion_descriptor_must_match_before_admission() {
    let fixture = Fixture::new().await;
    fixture.state.lock().unwrap().bad_completion = true;
    let directory = tempfile::tempdir().unwrap();
    let mut journal = journal(directory.path(), fixture.record(true, false));
    let error = run_now(&fixture.client(), &mut journal).await.unwrap_err();
    assert!(error.to_string().contains("completion descriptor changed"));
    assert!(fixture.state.lock().unwrap().submits.is_empty());
    assert!(!journal.record.submitted);
    fixture.state.lock().unwrap().bad_completion = false;
    assert!(matches!(
        run_now(&fixture.client(), &mut journal).await.unwrap(),
        Outcome::Complete(_)
    ));
    let status = journal.record.uploads[0].session.as_ref().unwrap();
    assert!(status.complete);
    assert_eq!(status.offset, status.object.size);
    fixture.close().await;
}

#[tokio::test]
async fn lost_completion_response_observes_the_same_complete_session() {
    let fixture = Fixture::new().await;
    fixture.state.lock().unwrap().lose_completion = true;
    let directory = tempfile::tempdir().unwrap();
    let mut journal = journal(directory.path(), fixture.record(true, false));
    assert!(matches!(
        run_now(&fixture.client(), &mut journal).await.unwrap(),
        Outcome::Complete(_)
    ));
    let session = journal.record.uploads[0].session.as_ref().unwrap();
    assert!(session.complete);
    {
        let state = fixture.state.lock().unwrap();
        assert_eq!(state.begin_count, 1);
        assert_eq!(state.uploads.len(), 1);
        assert!(state.uploads[&session.id].0.complete);
    }
    fixture.close().await;
}

#[tokio::test]
async fn wrong_publication_scope_and_denied_upload_are_not_accepted() {
    for denied in [true, false] {
        let fixture = Fixture::new().await;
        {
            let mut s = fixture.state.lock().unwrap();
            s.deny_upload = denied;
            s.bad_status = !denied;
        }
        let temporary = tempfile::tempdir().unwrap();
        let mut journal = journal(temporary.path(), fixture.record(false, false));
        assert!(run_now(&fixture.client(), &mut journal).await.is_err());
        assert!(journal.record.status.is_none());
        assert_eq!(fixture.state.lock().unwrap().submits.len(), usize::from(!denied));
        fixture.close().await;
    }
}
#[tokio::test]
async fn naming_failure_reports_active_identity_without_republishing_or_overwriting_later_names() {
    let fixture = Fixture::new().await;
    fixture.state.lock().unwrap().fail_naming = true;
    let temporary = tempfile::tempdir().unwrap();
    let mut journal = journal(temporary.path(), fixture.record(true, true));
    assert!(matches!(
        run_now(&fixture.client(), &mut journal).await.unwrap(),
        Outcome::NamingUnconfirmed(_)
    ));
    assert!(matches!(
        run_now(&fixture.client(), &mut journal).await.unwrap(),
        Outcome::NamingUnconfirmed(_)
    ));
    {
        let state = fixture.state.lock().unwrap();
        assert_eq!(state.names, 1);
        assert_eq!(state.submits.len(), 1);
    }
    fixture.close().await;
}
#[tokio::test]
async fn redirect_and_cross_origin_never_forward_credentials_without_exact_approval() {
    let first = Fixture::new().await;
    let foreign = Fixture::new().await;
    first.state.lock().unwrap().redirect = Some(format!("{}v1/containers/publish-permission", foreign.endpoint));
    assert!(first.client().capabilities().await.is_err());
    assert!(first.client().artifact_endpoint(&foreign.endpoint, None).is_err());
    assert!(first
        .client()
        .artifact_endpoint(&foreign.endpoint, Some(&first.endpoint))
        .is_err());
    assert!(first
        .client()
        .artifact_endpoint(&foreign.endpoint, Some(&foreign.endpoint))
        .is_ok());
    assert_eq!(foreign.state.lock().unwrap().authenticated, 0);
    first.close().await;
    foreign.close().await;
}
#[tokio::test]
async fn journal_locks_and_reverifies_local_bytes_without_persisting_credentials() {
    let fixture = Fixture::new().await;
    let temporary = tempfile::tempdir().unwrap();
    let mut journal = journal(temporary.path(), fixture.record(false, false));
    let path = journal.directory().to_owned();
    assert!(Journal::open(&path).is_err());
    let bytes = std::fs::read_to_string(path.join("publication.json")).unwrap();
    assert!(!bytes.contains("isolated-fixture-credential"));
    std::fs::write(
        path.join("module.blob"),
        vec![0; system_empty::empty().bytes.as_ref().len()],
    )
    .unwrap();
    assert!(run_now(&fixture.client(), &mut journal).await.is_err());
    assert_eq!(fixture.state.lock().unwrap().begin_count, 0);
    drop(journal);
    let mut value: serde_json::Value = serde_json::from_str(&bytes).unwrap();
    value["request_json"] = json!("{}");
    std::fs::write(path.join("publication.json"), serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(Journal::open(&path).is_err());
    fixture.close().await;
}
#[test]
fn endpoints_and_receipts_are_bounded_and_unambiguous() {
    for endpoint in [
        "Maincloud",
        "http://10.0.0.1",
        "https://user:password@example.invalid",
        "https://example.invalid/?token=secret",
    ] {
        assert!(client::endpoint(endpoint).is_err());
    }
    for endpoint in ["http://localhost:3000", "http://[::1]:3000"] {
        assert!(client::endpoint(endpoint).is_ok());
    }
    let object = ObjectRef {
        digest: empty_module_artifact().digest,
        size: 1,
    };
    let status = UploadStatus {
        id: uuid::Uuid::now_v7(),
        object,
        offset: 0,
        expires_at: 0,
        complete: false,
    };
    assert!(status.validate(object, None).is_err());
}

#[tokio::test]
async fn incomplete_artifact_retention_never_publishes_a_resume_record_or_mutates_server() {
    let fixture = Fixture::new().await;
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path().join("new").join("nested").join("state");
    let record = fixture.record(false, false);
    let id = record.request().unwrap().manifest.current().envelope.operation_id;
    // A missing promised module fails the artifact flush before publication.json
    // can make this operation available for reservation or admission.
    assert!(Journal::create(&base, record, None, None).is_err());
    assert!(!base.join(id.to_string()).exists());
    assert_eq!(fixture.state.lock().unwrap().authenticated, 0);
    fixture.close().await;
}

#[tokio::test]
async fn revoked_status_access_replays_original_put_without_missing_local_artifacts_or_new_uploads() {
    let fixture = Fixture::new().await;
    fixture.state.lock().unwrap().lose_submit_after_commit = true;
    let temporary = tempfile::tempdir().unwrap();
    let mut journal = journal(temporary.path(), fixture.record(false, false));
    let exact = journal.record.request_json.clone();
    assert!(run_now(&fixture.client(), &mut journal).await.is_err());
    let path = journal.directory().to_owned();
    drop(journal);
    std::fs::remove_file(path.join("module.blob")).unwrap();
    {
        let mut state = fixture.state.lock().unwrap();
        state.permission = false;
        state.deny_status = true;
        state.deny_upload = true;
    }
    let mut journal = Journal::open(&path).unwrap();
    assert!(matches!(
        run_now(&fixture.client(), &mut journal).await.unwrap(),
        Outcome::Complete(_)
    ));
    {
        let state = fixture.state.lock().unwrap();
        assert_eq!(state.submits, [exact.as_bytes(), exact.as_bytes()]);
        assert_eq!(state.begin_count, 1);
    }
    fixture.close().await;
}
