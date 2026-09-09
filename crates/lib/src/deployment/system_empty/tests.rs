use super::*;

fn declaration(name: &str, constraint: EnvironmentConstraint, optional: bool) -> EnvironmentDeclaration {
    EnvironmentDeclaration {
        name: name.into(),
        constraint,
        optional,
    }
}

#[test]
fn equivalent_normalized_schemas_select_the_same_program() {
    let first = EnvironmentSchema::new(vec![
        declaration("TOKEN", EnvironmentConstraint::AnyString, true),
        declaration(
            "MODE",
            EnvironmentConstraint::OneOf(vec!["blue".into(), "green".into()]),
            false,
        ),
    ])
    .unwrap();
    let second = EnvironmentSchema::new(vec![
        declaration(
            "MODE",
            EnvironmentConstraint::OneOf(vec!["green".into(), "blue".into(), "blue".into()]),
            false,
        ),
        declaration("TOKEN", EnvironmentConstraint::AnyString, true),
    ])
    .unwrap();
    let generated = generate(&first).unwrap();
    let same = generate(&second).unwrap();
    assert_eq!(generated.bytes, same.bytes);
    assert_eq!(generated.descriptor, same.descriptor);
    assert_eq!(verify(&generated.descriptor, &generated.bytes).unwrap(), first);
    let required = EnvironmentSchema::new(vec![declaration("TOKEN", EnvironmentConstraint::AnyString, false)]).unwrap();
    assert_ne!(
        generated.descriptor.program_hash,
        generate(&required).unwrap().descriptor.program_hash
    );
}

#[test]
fn recognition_rejects_old_versions_forged_hashes_and_extra_executable_bytes() {
    let generated = generate(&EnvironmentSchema::default()).unwrap();
    assert!(verify(&generated.descriptor, &generated.bytes).unwrap().is_empty());
    let mut descriptor = generated.descriptor;
    descriptor.version = 1;
    assert!(verify(&descriptor, &generated.bytes).is_err());
    descriptor = generated.descriptor;
    descriptor.program_hash = Hash::ZERO;
    assert!(verify(&descriptor, &generated.bytes).is_err());

    let mut bytes = generated.bytes.to_vec();
    // Even a valid Wasm custom section with a correctly claimed new hash fails
    // the exact platform-code check, rather than qualifying by schema alone.
    bytes.extend([0, 2, 1, b'x']);
    descriptor.program_hash = hash_bytes(&bytes);
    assert!(verify(&descriptor, &bytes).is_err());
    for length in 0..generated.bytes.len() {
        assert!(verify(&generated.descriptor, &generated.bytes[..length]).is_err());
    }
}

#[test]
fn large_declarations_grow_fixed_memory_and_remain_bounded() {
    let schema = EnvironmentSchema::new(
        (0..255)
            .map(|n| {
                declaration(
                    &format!("K{n}"),
                    EnvironmentConstraint::Literal("x".repeat(MAX_ENV_VALUE_BYTES)),
                    false,
                )
            })
            .collect(),
    )
    .unwrap();
    let generated = generate(&schema).unwrap();
    assert!(generated.bytes.len() > 2_000_000);
    assert!(generated.bytes.len() <= MAX_PROGRAM_BYTES);
    assert_eq!(verify(&generated.descriptor, &generated.bytes).unwrap(), schema);
    let mut wasm = Reader(&generated.bytes);
    wasm.expect(WASM_HEADER).unwrap();
    let mut pages = None;
    while !wasm.0.is_empty() {
        let tag = wasm.byte().unwrap();
        let length = wasm.leb().unwrap();
        let mut payload = Reader(wasm.take(length).unwrap());
        if tag == 5 {
            payload.expect(&[1, 1]).unwrap();
            let minimum = payload.leb().unwrap();
            let maximum = payload.leb().unwrap();
            assert_eq!(minimum, maximum);
            payload.end().unwrap();
            pages = Some(minimum);
        }
    }
    assert_eq!(pages, Some(32));
}

#[test]
fn hostile_counts_lengths_and_noncanonical_metadata_fail_before_allocation() {
    let generated = generate(&EnvironmentSchema::default()).unwrap();
    let mut bytes = generated.bytes.to_vec();
    let prefix = [2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 15];
    let offset = bytes.windows(prefix.len()).position(|window| window == prefix).unwrap() + prefix.len();
    bytes[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    let descriptor = SystemEmptyModule {
        version: VERSION,
        program_hash: hash_bytes(&bytes),
    };
    assert!(verify(&descriptor, &bytes).is_err());

    let mut bytes = WASM_HEADER.to_vec();
    bytes.extend([11, 0xff, 0xff, 0xff, 0xff, 0x0f]);
    assert!(verify(&descriptor, &bytes).is_err());
    let mut bytes = WASM_HEADER.to_vec();
    bytes.extend([11, 0xff, 0xff, 0xff, 0xff, 0x10]);
    assert!(verify(&descriptor, &bytes).is_err());
    let oversized = vec![0; MAX_PROGRAM_BYTES + 1];
    assert!(verify(&descriptor, &oversized).is_err());
}

#[test]
fn platform_imports_use_the_current_authorization_namespace() {
    let generated = empty();
    let mut wasm = Reader(&generated.bytes);
    wasm.expect(WASM_HEADER).unwrap();
    let mut imports = None;
    while !wasm.0.is_empty() {
        let tag = wasm.byte().unwrap();
        let length = wasm.leb().unwrap();
        let payload = wasm.take(length).unwrap();
        if tag == 2 {
            assert!(imports.replace(payload).is_none());
        }
    }
    let mut expected = vec![2];
    name(&mut expected, b"spacetime_10.0");
    name(&mut expected, b"bytes_sink_write");
    expected.extend([0, 0]);
    name(&mut expected, b"spacetime_10.7");
    name(&mut expected, b"get_call_auth_flags");
    expected.extend([0, 1]);
    assert_eq!(imports, Some(expected.as_slice()));

    // A correctly hashed prototype namespace cannot identify the current
    // platform module, even when declarations and descriptor version match.
    let mut old = generated.bytes.to_vec();
    let namespace = b"spacetime_10.7";
    let offset = old
        .windows(namespace.len())
        .position(|bytes| bytes == namespace)
        .unwrap();
    old[offset + namespace.len() - 1] = b'6';
    let descriptor = SystemEmptyModule {
        version: VERSION,
        program_hash: hash_bytes(&old),
    };
    assert!(verify(&descriptor, &old).is_err());
}
