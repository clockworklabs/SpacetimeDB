use super::*;

fn operation_id() -> Uuid {
    Uuid::parse_str("01991ec4-0000-7000-8000-000000000001").unwrap()
}

fn request(module_action: ModuleAction) -> PublishEnvelope {
    PublishEnvelope {
        version: PUBLISH_PROTOCOL_VERSION,
        operation_id: operation_id(),
        expected_revision: None,
        module_action,
        container_action: ContainerAction::Keep,
    }
}

#[test]
fn component_actions_preserve_or_explicitly_remove_the_module() {
    let limits = ContainerSpecLimits::default();
    let user = UserModule {
        kind: UserModuleKind::Wasm,
        program_hash: hash_bytes(b"valid module artifact"),
    };
    let set = request(ModuleAction::Set(user.clone())).resolve(None, &limits).unwrap();
    assert_eq!(set.current().module, ModuleComponent::User(user));
    let keep = request(ModuleAction::Keep).resolve(Some(&set), &limits).unwrap();
    assert_eq!(set, keep);
    let remove = request(ModuleAction::Remove).resolve(Some(&set), &limits).unwrap();
    assert_eq!(
        remove.current().module,
        ModuleComponent::SystemEmpty(SYSTEM_EMPTY_MODULE_VERSION)
    );
    assert_ne!(remove.revision().unwrap(), keep.revision().unwrap());
}

#[test]
fn concrete_module_actions_preserve_wire_tags_and_export_distinct_names() {
    let module = UserModule {
        kind: UserModuleKind::Js,
        program_hash: hash_bytes(b"module"),
    };
    assert_eq!(bsatn::to_vec(&ModuleAction::Keep).unwrap(), [0]);
    assert_eq!(bsatn::to_vec(&ModuleAction::Remove).unwrap(), [2]);
    let mut expected = vec![1];
    expected.extend(bsatn::to_vec(&module).unwrap());
    assert_eq!(bsatn::to_vec(&ModuleAction::Set(module)).unwrap(), expected);

    use crate::db::raw_def::v11::{RawModuleDefV11Builder, RawModuleDefV11Section};
    let mut builder = RawModuleDefV11Builder::new();
    builder.add_type::<PublishEnvelope>();
    let raw = builder.finish();
    let names: Vec<_> = raw
        .sections
        .iter()
        .filter_map(|section| match section {
            RawModuleDefV11Section::Types(types) => Some(types),
            _ => None,
        })
        .flatten()
        .map(|ty| &*ty.source_name.source_name)
        .collect();
    let distinct: std::collections::BTreeSet<_> = names.iter().copied().collect();
    assert_eq!(
        names.len(),
        distinct.len(),
        "publish envelope exports duplicate type names"
    );
    assert!(distinct.contains("ModuleAction"));
    assert!(distinct.contains("ContainerAction"));
}

#[test]
fn unknown_deployments_do_not_fall_back_to_an_empty_module() {
    assert!(DeploymentSpec::decode(&[255]).is_err());
    let mut envelope = request(ModuleAction::Keep);
    envelope.version += 1;
    assert!(matches!(
        envelope.resolve(None, &ContainerSpecLimits::default()),
        Err(DeploymentValidationError::UnsupportedVersion)
    ));
    let spec = DeploymentSpec::V1(DeploymentSpecV1 {
        module: ModuleComponent::SystemEmpty(9000),
        container: None,
    });
    assert!(matches!(
        spec.normalize(&ContainerSpecLimits::default()),
        Err(DeploymentValidationError::UnsupportedEmptyModule)
    ));
}

#[test]
fn operation_age_is_enforced_even_when_no_ledger_row_remains() {
    let id = operation_id();
    let created_ms = (id.as_u128() >> 80) as u64;
    assert_eq!(
        operation_expiry_ms(id, created_ms).unwrap(),
        created_ms + PUBLISH_RETRY_WINDOW_MS
    );
    assert!(matches!(
        operation_expiry_ms(id, created_ms + PUBLISH_RETRY_WINDOW_MS),
        Err(DeploymentValidationError::ExpiredOperation)
    ));
    assert!(matches!(
        operation_expiry_ms(id, created_ms - MAX_OPERATION_CLOCK_SKEW_MS - 1),
        Err(DeploymentValidationError::FutureOperation)
    ));
    assert!(matches!(
        operation_expiry_ms(Uuid::NIL, created_ms),
        Err(DeploymentValidationError::InvalidOperationId)
    ));
}

#[cfg(feature = "serde")]
#[test]
fn uuid_json_is_lossless_and_omission_means_keep() {
    let envelope = request(ModuleAction::Keep);
    let mut json = serde_json::to_value(&envelope).unwrap();
    assert_eq!(json["operation_id"], operation_id().to_string());
    json.as_object_mut().unwrap().remove("module_action");
    json.as_object_mut().unwrap().remove("container_action");
    let parsed: PublishEnvelope = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(parsed, envelope);
    assert!(!parsed.requires_container_permission());
    json["publisher_identity"] = "untrusted owner override".into();
    assert!(serde_json::from_value::<PublishEnvelope>(json).is_err());
    let binary = bsatn::to_vec(&envelope).unwrap();
    assert_eq!(bsatn::from_slice::<PublishEnvelope>(&binary).unwrap(), envelope);
}
