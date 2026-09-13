use super::*;

fn operation_id() -> Uuid {
    Uuid::parse_str("01991ec4-0000-7000-8000-000000000001").unwrap()
}

fn request(module_action: ModuleAction) -> PublishEnvelope {
    PublishEnvelope {
        version: PUBLISH_PROTOCOL_VERSION,
        operation_id: operation_id(),
        expected_revision: None,
        expected_last_operation: None,
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
    let remove = request(ModuleAction::Remove(system_empty::empty().descriptor))
        .resolve(Some(&set), &limits)
        .unwrap();
    assert_eq!(
        remove.current().module,
        ModuleComponent::SystemEmpty(crate::deployment::system_empty::empty().descriptor)
    );
    assert_ne!(remove.revision().unwrap(), keep.revision().unwrap());
}

#[test]
fn declared_platform_schema_changes_revision_and_keep_retains_its_program() {
    use crate::environment::{EnvironmentConstraint, EnvironmentDeclaration, EnvironmentSchema};
    let initial = request(ModuleAction::Keep).resolve(None, &Default::default()).unwrap();
    let schema = EnvironmentSchema::new(vec![EnvironmentDeclaration {
        name: "TOKEN".into(),
        constraint: EnvironmentConstraint::AnyString,
        optional: false,
    }])
    .unwrap();
    let configured = system_empty::generate(&schema).unwrap();
    let selected = request(ModuleAction::Remove(configured.descriptor))
        .resolve(Some(&initial), &Default::default())
        .unwrap();
    assert_eq!(
        selected.current().module,
        ModuleComponent::SystemEmpty(configured.descriptor)
    );
    assert_ne!(selected.revision().unwrap(), initial.revision().unwrap());
    assert_eq!(
        request(ModuleAction::Keep)
            .resolve(Some(&selected), &Default::default())
            .unwrap(),
        selected
    );
}

#[test]
fn concrete_module_actions_preserve_wire_tags_and_export_distinct_names() {
    let module = UserModule {
        kind: UserModuleKind::Js,
        program_hash: hash_bytes(b"module"),
    };
    assert_eq!(bsatn::to_vec(&ModuleAction::Keep).unwrap(), [0]);
    let mut removed = vec![2];
    removed.extend(bsatn::to_vec(&system_empty::empty().descriptor).unwrap());
    assert_eq!(
        bsatn::to_vec(&ModuleAction::Remove(system_empty::empty().descriptor)).unwrap(),
        removed
    );
    let mut expected = vec![1];
    expected.extend(bsatn::to_vec(&module).unwrap());
    assert_eq!(bsatn::to_vec(&ModuleAction::Set(module)).unwrap(), expected);

    use crate::db::raw_def::v10::{RawModuleDefV10Builder, RawModuleDefV10Section};
    let mut builder = RawModuleDefV10Builder::new();
    builder.add_type::<PublishEnvelope>();
    let raw = builder.finish();
    let names: Vec<_> = raw
        .sections
        .iter()
        .filter_map(|section| match section {
            RawModuleDefV10Section::Types(types) => Some(types),
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
        module: ModuleComponent::SystemEmpty(SystemEmptyModule {
            version: 9000,
            program_hash: Hash::ZERO,
        }),
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
