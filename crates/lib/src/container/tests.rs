use super::*;

fn spec() -> ContainerSpec {
    ContainerSpec {
        image_manifest: OciDigest::sha256([7; 32]),
        image_platform: ImagePlatform {
            os: "linux".into(),
            architecture: "arm64".into(),
        },
        argv: vec!["/app/server".into()],
        user: "1000:1000".into(),
        working_directory: "/app".into(),
        mode: ContainerMode::Service,
        restart: RestartPolicy::OnFailure,
        env_keys: vec!["API_KEY".into(), "DATABASE_URL".into()],
        resources: ContainerResources {
            cpu_millicores: 1000,
            memory_bytes: 64 * 1024 * 1024,
            scratch_bytes: 64 * 1024 * 1024,
            pids_max: 64,
        },
        ports: vec![ContainerPort {
            name: "http".into(),
            port: 8080,
            protocol: PortProtocol::Http,
            exposure: PortExposure::Public,
            readiness_probe: ReadinessProbe::default(),
        }],
        mounts: vec![],
        stop_grace_ms: DEFAULT_STOP_GRACE_MS,
    }
}

#[test]
fn oci_digest_is_strict_and_cannot_be_used_for_path_traversal() {
    let digest = format!("sha256:{}", "ab".repeat(32));
    assert_eq!(digest.parse::<OciDigest>().unwrap().to_string(), digest);
    for value in [
        "sha256:../../host",
        "sha256:abc",
        "sha512:abc",
        "blake3:abc",
        "SHA256:abc",
        "sha256:",
    ] {
        assert!(value.parse::<OciDigest>().is_err(), "{value}");
    }
    assert!(format!("sha256:{}", "AB".repeat(32)).parse::<OciDigest>().is_err());
    assert!(format!("sha256:{}\n", "ab".repeat(32)).parse::<OciDigest>().is_err());
}

#[test]
fn canonical_hash_ignores_declaration_order_but_preserves_launch_semantics() {
    let limits = ContainerSpecLimits::default();
    let mut original = spec();
    original.ports.push(ContainerPort {
        name: "admin".into(),
        port: 8081,
        ..original.ports[0].clone()
    });
    let hash = original.canonical_hash(&limits).unwrap();
    let mut reordered = original.clone();
    reordered.env_keys.reverse();
    reordered.ports.reverse();
    assert_eq!(reordered.canonical_hash(&limits).unwrap(), hash);
    let mut changed = reordered;
    changed.argv.push("--read-only".into());
    assert_ne!(changed.canonical_hash(&limits).unwrap(), hash);
    assert_ne!(
        ContainerSpec {
            resources: ContainerResources {
                pids_max: 63,
                ..original.resources
            },
            ..original.clone()
        }
        .canonical_hash(&limits)
        .unwrap(),
        hash
    );
    let bytes = bsatn::to_vec(&original).unwrap();
    assert_eq!(bsatn::from_slice::<ContainerSpec>(&bytes).unwrap(), original);
}

#[test]
fn duplicate_declarations_are_not_silently_deduplicated() {
    let limits = ContainerSpecLimits::default();
    let mut value = spec();
    value.env_keys.push(value.env_keys[0].clone());
    assert_eq!(value.normalize(&limits).unwrap_err().field, "env_keys");
    let mut value = spec();
    value.ports.push(ContainerPort {
        name: "another".into(),
        ..value.ports[0].clone()
    });
    assert_eq!(value.normalize(&limits).unwrap_err().field, "ports");
    let mut value = spec();
    value.ports.push(ContainerPort {
        port: 9090,
        ..value.ports[0].clone()
    });
    assert_eq!(value.normalize(&limits).unwrap_err().field, "ports");
}

#[test]
fn readiness_cannot_change_authority_or_inject_a_request() {
    for path in [
        "https://internal/",
        "//internal/",
        "/\\internal/",
        "/health\r\nHost: internal",
        "/health#fragment",
    ] {
        let mut value = spec();
        value.ports[0].readiness_probe = ReadinessProbe::Http(HttpProbe {
            path: path.into(),
            timeout_ms: 1000,
            interval_ms: 5000,
        });
        assert_eq!(
            value.validate(&ContainerSpecLimits::default()).unwrap_err().field,
            "readiness_probe.path"
        );
    }
    let mut value = spec();
    value.ports[0].readiness_probe = ReadinessProbe::Http(HttpProbe {
        path: "/health?full=1".into(),
        timeout_ms: 1000,
        interval_ms: 5000,
    });
    value.validate(&ContainerSpecLimits::default()).unwrap();
}

#[test]
fn stage_one_rejects_mounts_and_unsupported_platforms() {
    let mut value = spec();
    value.mounts.push(ContainerMount {
        database: "self".into(),
        source: "/".into(),
        target: "/spacetime".into(),
        read_only: false,
    });
    assert_eq!(
        value.validate(&ContainerSpecLimits::default()).unwrap_err().field,
        "mounts"
    );
    let mut value = spec();
    value.image_platform.os = "windows".into();
    assert_eq!(
        value.validate(&ContainerSpecLimits::default()).unwrap_err().field,
        "image_platform"
    );
}

#[test]
fn startup_size_counts_environment_and_pointers_without_leaking_values() {
    let secret = "THIS_VALUE_MUST_NOT_APPEAR_IN_ERRORS";
    let env = vec![format!("KEY={secret}\0")];
    let error = validate_exec_size(&spec().argv, &env).unwrap_err().to_string();
    assert!(!error.contains(secret));
    assert!(!error.contains("KEY="));
    let bounded_strings = vec!["x".repeat(MAX_EXEC_STRING_BYTES - 1); 5];
    assert!(validate_exec_size(&spec().argv, &bounded_strings).is_err());
    let many_empty_entries = vec![String::new(); MAX_EXEC_BYTES / 8];
    assert!(validate_exec_size(&[], &many_empty_entries).is_err());
    assert!(validate_exec_size(&spec().argv, &["KEY=value".into()]).is_ok());
}

#[test]
fn tenant_environment_cannot_override_platform_discovery_or_credentials() {
    for key in [
        "SPACETIMEDB_DATABASE_IDENTITY",
        "SPACETIMEDB_SERVER_URI",
        "SPACETIMEDB_CREDENTIAL_BROKER",
        "SPACETIMEDB_TOKEN",
        "A=B",
        "0BAD",
        "BAD\0KEY",
    ] {
        assert!(validate_env_key(key).is_err(), "{key:?}");
    }
    for key in ["API_KEY", "_CUSTOM", "path", "A1"] {
        validate_env_key(key).unwrap();
    }
}

#[test]
fn zero_or_overflowing_resource_requests_are_not_admitted() {
    let limits = ContainerSpecLimits::default();
    for cpu in [0, u64::MAX, limits.resources.cpu_millicores + 1] {
        let mut value = spec();
        value.resources.cpu_millicores = cpu;
        assert_eq!(value.validate(&limits).unwrap_err().field, "resources");
    }
    let mut value = spec();
    value.resources.scratch_bytes = 0;
    assert!(value.validate(&limits).is_err());
}

#[test]
fn successful_jobs_and_on_failure_services_remain_terminal() {
    assert!(!RestartPolicy::Never.restarts_after(None));
    assert!(!RestartPolicy::Never.restarts_after(Some(1)));
    assert!(!RestartPolicy::OnFailure.restarts_after(Some(0)));
    assert!(RestartPolicy::OnFailure.restarts_after(None));
    assert!(RestartPolicy::OnFailure.restarts_after(Some(1)));
    let mut job = spec();
    job.mode = ContainerMode::Job;
    job.restart = RestartPolicy::Always;
    assert_eq!(
        job.validate(&ContainerSpecLimits::default()).unwrap_err().field,
        "restart"
    );
}

#[cfg(feature = "serde")]
#[test]
fn json_requires_explicit_port_exposure_and_rejects_privileged_fields() {
    let original = spec();
    let json = serde_json::to_value(&original).unwrap();
    assert_eq!(serde_json::from_value::<ContainerSpec>(json.clone()).unwrap(), original);
    let mut missing = json.clone();
    missing["ports"][0].as_object_mut().unwrap().remove("exposure");
    assert!(serde_json::from_value::<ContainerSpec>(missing).is_err());
    let mut injected = json;
    injected
        .as_object_mut()
        .unwrap()
        .insert("privileged".into(), true.into());
    assert!(serde_json::from_value::<ContainerSpec>(injected).is_err());
    let keep: ContainerAction = serde_json::from_str(r#"{"action":"keep"}"#).unwrap();
    assert_eq!(keep, ContainerAction::Keep);
    let remove: ContainerAction = serde_json::from_str(r#"{"action":"remove"}"#).unwrap();
    assert_eq!(remove, ContainerAction::Remove);
    assert!(serde_json::from_str::<ContainerAction>(r#"{"action":"remove","value":{}}"#).is_err());
}

#[test]
fn concrete_container_actions_preserve_wire_tags() {
    let container = spec();
    assert_eq!(bsatn::to_vec(&ContainerAction::Keep).unwrap(), [0]);
    assert_eq!(bsatn::to_vec(&ContainerAction::Remove).unwrap(), [2]);
    let mut expected = vec![1];
    expected.extend(bsatn::to_vec(&container).unwrap());
    assert_eq!(bsatn::to_vec(&ContainerAction::Set(container)).unwrap(), expected);
}
