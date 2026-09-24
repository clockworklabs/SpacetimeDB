use super::*;
use serde_json::{json, Value};

fn platform() -> ImagePlatform {
    ImagePlatform {
        os: "linux".into(),
        architecture: "arm64".into(),
    }
}
fn config() -> Value {
    json!({"architecture":"arm64","os":"linux","config":{
        "Entrypoint":["node"],"Cmd":["server.js"],"User":"1000:1000","WorkingDir":"/app",
        "Env":["PATH=/usr/bin","APP_KEY=image-secret"]
    },"rootfs":{"type":"layers","diff_ids":[sha256(b"uncompressed layer")]}})
}
fn fixture(config: &Value) -> (Vec<u8>, Vec<u8>, Descriptor) {
    let config_bytes = serde_json::to_vec(config).unwrap();
    let manifest = serde_json::to_vec(&json!({
        "schemaVersion":2,"mediaType":OCI_MANIFEST,
        "config":{"mediaType":OCI_CONFIG,"digest":sha256(&config_bytes),"size":config_bytes.len()},
        "layers":[{"mediaType":"application/vnd.oci.image.layer.v1.tar+gzip","digest":sha256(b"layer"),"size":5}]
    }))
    .unwrap();
    let descriptor = Descriptor {
        media_type: OCI_MANIFEST.into(),
        digest: sha256(&manifest),
        size: manifest.len() as u64,
        platform: Some(Platform {
            os: "linux".into(),
            architecture: "arm64".into(),
            variant: Some("v8".into()),
            os_version: None,
            os_features: vec![],
        }),
        urls: vec![],
        data: None,
        artifact_type: None,
    };
    (config_bytes, manifest, descriptor)
}

#[test]
fn immutable_image_preserves_defaults_and_explicit_command_replaces_all_argv() {
    let (bytes, raw, descriptor) = fixture(&config());
    verify_object(&descriptor, &raw).unwrap();
    let manifest = parse_manifest(&raw).unwrap();
    let image = parse_config(&bytes, &manifest, &platform()).unwrap();
    assert_eq!(image.config.argv(None).unwrap(), ["node", "server.js"]);
    assert_eq!(image.config.argv(Some(&["/bin/sh".into()])).unwrap(), ["/bin/sh"]);
    assert_eq!(image.config.user, "1000:1000");
    assert_eq!(image.config.working_directory, "/app");
    assert_eq!(image.config.environment().unwrap()["APP_KEY"], "image-secret");
    assert!(!format!("{image:?}").contains("image-secret"));
    assert_eq!(object_closure(descriptor, &manifest).unwrap().len(), 3);
}

#[test]
fn content_verification_checks_actual_sha256_and_length() {
    assert_eq!(
        sha256(b"abc").to_string(),
        "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let (_, bytes, mut descriptor) = fixture(&config());
    assert!(verify_object(&descriptor, b"different").is_err());
    descriptor.size -= 1;
    assert!(verify_object(&descriptor, &bytes).is_err());
}

#[test]
fn platform_selection_ignores_attestations_and_rejects_ambiguity_or_absence() {
    let (_, _, descriptor) = fixture(&config());
    let mut attestation = descriptor.clone();
    attestation.platform = Some(Platform {
        os: "unknown".into(),
        architecture: "unknown".into(),
        variant: None,
        os_version: None,
        os_features: vec![],
    });
    let index = |descriptors: Vec<Descriptor>| {
        serde_json::to_vec(&json!({"schemaVersion":2,"mediaType":OCI_INDEX,"manifests":descriptors})).unwrap()
    };
    assert_eq!(
        select_platform(&index(vec![attestation.clone(), descriptor.clone()]), &platform()).unwrap(),
        descriptor
    );
    assert!(select_platform(&index(vec![descriptor.clone(), descriptor]), &platform()).is_err());
    assert!(select_platform(&index(vec![attestation]), &platform()).is_err());
}

#[test]
fn foreign_layers_urls_inline_data_and_artifacts_fail_closed() {
    let (_, bytes, _) = fixture(&config());
    let baseline: Value = serde_json::from_slice(&bytes).unwrap();
    for (key, value) in [
        ("urls", json!(["http://169.254.169.254/latest/meta-data/"])),
        ("data", json!("inline")),
        ("artifactType", json!("application/test")),
    ] {
        let mut manifest = baseline.clone();
        manifest["layers"][0][key] = value;
        assert!(parse_manifest(&serde_json::to_vec(&manifest).unwrap()).is_err());
    }
    let mut manifest = baseline.clone();
    manifest["layers"][0]["mediaType"] = json!("application/vnd.docker.image.rootfs.foreign.diff.tar.gzip");
    assert!(parse_manifest(&serde_json::to_vec(&manifest).unwrap()).is_err());
    let mut manifest = baseline;
    manifest["artifactType"] = json!("application/test");
    assert!(parse_manifest(&serde_json::to_vec(&manifest).unwrap()).is_err());
}

#[test]
fn config_rejects_volumes_wrong_platform_rootfs_and_reserved_environment() {
    for (field, value) in [
        ("Volumes", json!({"/escape":{}})),
        ("Env", json!(["SPACETIMEDB_IDENTITY=forged"])),
        ("Env", json!(["KEY=a", "KEY=b"])),
        ("Env", json!(["NO_EQUALS"])),
        ("Env", json!(["KEY=contains\u{0}nul"])),
        ("WorkingDir", json!("relative/path")),
    ] {
        let mut config = config();
        config["config"][field] = value;
        let (bytes, raw, _) = fixture(&config);
        assert!(
            parse_config(&bytes, &parse_manifest(&raw).unwrap(), &platform()).is_err(),
            "{field}"
        );
    }
    for (field, value) in [
        ("architecture", json!("amd64")),
        ("rootfs", json!({"type":"layers","diff_ids":[]})),
    ] {
        let mut config = config();
        config[field] = value;
        let (bytes, raw, _) = fixture(&config);
        assert!(parse_config(&bytes, &parse_manifest(&raw).unwrap(), &platform()).is_err());
    }
}

#[test]
fn size_limits_and_conflicting_descriptors_are_enforced() {
    let (_, bytes, descriptor) = fixture(&config());
    let mut manifest: Value = serde_json::from_slice(&bytes).unwrap();
    manifest["layers"][0]["size"] = json!(MAX_IMAGE_BYTES);
    assert!(parse_manifest(&serde_json::to_vec(&manifest).unwrap()).is_err());
    assert!(parse_manifest(&vec![b' '; MAX_MANIFEST_BYTES + 1]).is_err());
    let mut manifest = parse_manifest(&bytes).unwrap();
    let mut conflicting = manifest.layers[0].clone();
    conflicting.size += 1;
    manifest.layers.push(conflicting);
    assert!(object_closure(descriptor, &manifest).is_err());
}

#[test]
fn empty_scratch_image_requires_explicit_main_command() {
    let config = ContainerConfig::default();
    assert!(config.argv(None).is_err());
    assert!(config.argv(Some(&[])).is_err());
    assert!(config.argv(Some(&["".into()])).is_err());
    assert_eq!(config.argv(Some(&["/main".into()])).unwrap(), ["/main"]);
    assert!(valid_env_key("_A0"));
    assert!(!valid_env_key("0A"));
    assert!(valid_env_key(&"A".repeat(256)));
    assert!(!valid_env_key(&"A".repeat(257)));
}
