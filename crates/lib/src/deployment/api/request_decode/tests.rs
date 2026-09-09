use super::*;
use crate::container::{ContainerAction, OciDigest};
use crate::deployment::manifest::{
    ModuleArtifact, PreparedDeploymentManifest, PreparedDeploymentManifestV1, PreparedMigrationPolicy,
};
use crate::deployment::{
    DeploymentSpec, DeploymentSpecV1, ModuleAction, ModuleComponent, PublishEnvelope, PUBLISH_PROTOCOL_VERSION,
};
use crate::Uuid;

fn request() -> PublishRequest {
    PublishRequest {
        manifest: PreparedDeploymentManifest::V1(PreparedDeploymentManifestV1 {
            envelope: PublishEnvelope {
                version: PUBLISH_PROTOCOL_VERSION,
                operation_id: Uuid::parse_str("01991ec4-0000-7000-8000-000000000001").unwrap(),
                expected_revision: None,
                expected_last_operation: None,
                module_action: ModuleAction::Keep,
                container_action: ContainerAction::Keep,
            },
            deployment: DeploymentSpec::V1(DeploymentSpecV1 {
                module: ModuleComponent::SystemEmpty(crate::deployment::system_empty::empty().descriptor),
                container: None,
            }),
            module_artifact: ModuleArtifact {
                digest: OciDigest::sha256([17; 32]),
                size_bytes: 250,
            },
            migration_policy: PreparedMigrationPolicy::Compatible,
        }),
        creation: None,
        image_source: None,
        environment: BTreeMap::new(),
    }
}

fn with_raw_environment(raw: &str) -> Vec<u8> {
    let mut value = serde_json::to_value(request()).unwrap();
    value.as_object_mut().unwrap().remove("environment");
    let mut json = serde_json::to_string(&value).unwrap();
    json.pop();
    format!("{json},\"environment\":{raw}}}").into_bytes()
}

#[test]
fn protected_environment_decode_is_complete_redacted_and_preserves_strings() {
    let mut original = request();
    original.environment = BTreeMap::from([
        ("TOKEN".into(), "private-secret-sentinel".into()),
        ("EMPTY".into(), String::new()),
        ("NUL".into(), "a\0b".into()),
        ("UTF8".into(), "日本語".into()),
    ]);
    let bytes = serde_json::to_vec(&original).unwrap();
    assert_eq!(PublishRequest::decode(&bytes).unwrap(), original);
    let debug = format!("{original:?}");
    assert!(debug.contains("[redacted]"));
    for (key, value) in &original.environment {
        assert!(!debug.contains(key));
        if !value.is_empty() {
            assert!(!debug.contains(value));
        }
    }
    let mut without = serde_json::to_value(original).unwrap();
    without.as_object_mut().unwrap().remove("environment");
    assert!(PublishRequest::decode(&serde_json::to_vec(&without).unwrap())
        .unwrap()
        .environment
        .is_empty());
    assert!(PublishRequest::decode(&with_raw_environment("{}"))
        .unwrap()
        .environment
        .is_empty());
}

#[test]
fn duplicate_malformed_and_invalid_environment_errors_never_echo_input() {
    for raw in [
        r#"{"TOKEN":"private-secret-sentinel","TOKEN":"other"}"#,
        r#"{"TOKEN":"private-secret-sentinel","\u0054OKEN":"other"}"#,
        r#"{"TOKEN":{"private-secret-sentinel":true}}"#,
        r#"{"private-secret-sentinel":"value"}"#,
        r#"{"TOKEN":123}"#,
        r#"{"TOKEN":"private-secret-sentinel""#,
        "null",
        "[]",
    ] {
        let error = PublishRequest::decode(&with_raw_environment(raw)).unwrap_err();
        assert_eq!(error.to_string(), "invalid publication request");
        assert_eq!(format!("{error:?}"), "PublishRequestError");
    }
}

#[test]
fn environment_and_raw_byte_limits_are_independent_of_manifest_size() {
    let mut original = request();
    for index in 0..MAX_ENV_VARS {
        let key = format!("K{index:03}{}", "K".repeat(MAX_ENV_KEY_BYTES - 4));
        original.environment.insert(key, "\0".repeat(MAX_ENV_VALUE_BYTES));
    }
    let bytes = serde_json::to_vec(&original).unwrap();
    assert!(bytes.len() <= MAX_PUBLISH_REQUEST_BYTES);
    assert_eq!(PublishRequest::decode(&bytes).unwrap(), original);
    original.environment.insert("EXTRA".into(), String::new());
    assert!(PublishRequest::decode(&serde_json::to_vec(&original).unwrap()).is_err());
    original.environment.clear();
    for (key, value) in [
        ("K".repeat(MAX_ENV_KEY_BYTES + 1), String::new()),
        ("K".into(), "é".repeat(MAX_ENV_VALUE_BYTES / 2 + 1)),
    ] {
        original.environment = BTreeMap::from([(key, value)]);
        assert!(original.validate_structure().is_err());
        assert!(PublishRequest::decode(&serde_json::to_vec(&original).unwrap()).is_err());
    }
    assert_eq!(
        PublishRequest::decode(&vec![b' '; MAX_PUBLISH_REQUEST_BYTES + 1]),
        Err(PublishRequestError)
    );
    original.environment.clear();
    let PreparedDeploymentManifest::V1(manifest) = &mut original.manifest;
    let DeploymentSpec::V1(deployment) = &mut manifest.deployment;
    // Metadata retains its own byte cap even with an empty environment.
    use crate::container::*;
    let container = ContainerSpec {
        image_manifest: OciDigest::sha256([7; 32]),
        image_platform: ImagePlatform {
            os: "linux".into(),
            architecture: "arm64".into(),
        },
        argv: vec!["x".repeat(MAX_DEPLOYMENT_BYTES + 1)],
        user: "1000:1000".into(),
        working_directory: "/app".into(),
        mode: ContainerMode::Job,
        restart: RestartPolicy::Never,
        env_keys: vec![],
        resources: ContainerResources {
            cpu_millicores: 1000,
            memory_bytes: 64 * 1024 * 1024,
            scratch_bytes: 64 * 1024 * 1024,
            pids_max: 64,
        },
        ports: vec![],
        mounts: vec![],
        stop_grace_ms: DEFAULT_STOP_GRACE_MS,
    };
    deployment.container = Some(container);
    assert!(PublishRequest::decode(&serde_json::to_vec(&original).unwrap()).is_err());
}

#[test]
fn optional_publication_uuid_is_a_string_or_null_and_not_an_integer() {
    let mut original = request();
    let PreparedDeploymentManifest::V1(manifest) = &mut original.manifest;
    manifest.envelope.expected_last_operation = Some(manifest.envelope.operation_id);
    let value = serde_json::to_value(&original).unwrap();
    let cursor = &value["manifest"]["manifest"]["envelope"]["expected_last_operation"];
    assert_eq!(cursor.as_str(), Some("01991ec4-0000-7000-8000-000000000001"));
    assert_eq!(
        PublishRequest::decode(&serde_json::to_vec(&value).unwrap()).unwrap(),
        original
    );
    let mut value = value;
    value["manifest"]["manifest"]["envelope"]["expected_last_operation"] = 17.into();
    assert!(PublishRequest::decode(&serde_json::to_vec(&value).unwrap()).is_err());
}
