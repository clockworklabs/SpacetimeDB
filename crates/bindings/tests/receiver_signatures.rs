use spacetimedb::spacetimedb_lib::db::raw_def::v10::{
    CaseConversionPolicy, ExplicitNames, RawModuleDefV10, RawModuleDefV10Builder, RawModuleDefV10Section,
};
use spacetimedb::spacetimedb_lib::db::raw_def::v9::direct;
use spacetimedb::spacetimedb_lib::{AlgebraicType, ProductType};
use spacetimedb_schema::def::{reducer_signature_hash, ModuleDef};

fn receiver_module(policy: CaseConversionPolicy, explicit_name: bool, unrelated_type: bool) -> RawModuleDefV10 {
    let mut builder = RawModuleDefV10Builder::new();
    builder.set_case_conversion_policy(policy);
    if unrelated_type {
        builder.add_algebraic_type(
            [],
            "Unused",
            AlgebraicType::product([("Value", AlgebraicType::Bool)]),
            true,
        );
    }
    let state = builder.add_algebraic_type([], "State", AlgebraicType::unit(), true);
    let operation = builder.add_algebraic_type(
        [],
        "StateOp",
        AlgebraicType::sum([
            ("SetState", AlgebraicType::Ref(state)),
            ("ClearState", AlgebraicType::unit()),
        ]),
        true,
    );
    builder.add_reducer(
        "receivePing",
        ProductType::from([
            ("MessageBody", AlgebraicType::String),
            ("StateOps", AlgebraicType::array(AlgebraicType::Ref(operation))),
        ]),
    );
    builder
        .build_table_with_new_type(
            "outboundPing",
            ProductType::from([
                ("id", AlgebraicType::U64),
                ("target", AlgebraicType::identity()),
                ("MessageBody", AlgebraicType::String),
                ("StateOps", AlgebraicType::array(AlgebraicType::Ref(operation))),
            ]),
            true,
        )
        .with_auto_inc_primary_key(0)
        .with_index_no_accessor_name(direct(0), "outbound_ping_id_idx")
        .finish();
    builder.add_local_outbox("outboundPing", "receivePing", 1, [2.into(), 3.into()], None::<&str>);
    if explicit_name {
        let mut names = ExplicitNames::default();
        names.insert_function("receivePing", "PreviousWireName");
        names.insert_function("receivePing", "WirePing");
        builder.add_explicit_names(names);
    }
    let mut raw = builder.finish();
    for section in &mut raw.sections {
        if let RawModuleDefV10Section::Typespace(typespace) = section {
            typespace[state] = AlgebraicType::product([
                ("ItemID", AlgebraicType::U64),
                ("OwnerIdentity", AlgebraicType::identity()),
                ("Notes", AlgebraicType::option(AlgebraicType::String)),
                ("Children", AlgebraicType::array(AlgebraicType::Ref(state))),
            ]);
        }
    }
    raw
}

#[test]
fn local_outbox_signatures_use_validated_reducers() {
    for policy in [CaseConversionPolicy::None, CaseConversionPolicy::SnakeCase] {
        for explicit_name in [false, true] {
            let mut hashes = Vec::new();
            for unrelated_type in [false, true] {
                let raw = receiver_module(policy, explicit_name, unrelated_type);
                assert!(raw.outboxes().is_none());
                assert_eq!(
                    raw.local_outboxes().unwrap()[0].reducer_source_name.as_ref(),
                    "receivePing"
                );
                let validated: ModuleDef = raw.try_into().unwrap();
                let reducer = validated
                    .reducers()
                    .find(|reducer| reducer.accessor_name.as_ref() == "receivePing")
                    .unwrap();
                let outbox = validated.tables().find_map(|table| table.outbox.as_ref()).unwrap();
                assert_eq!(outbox.remote_reducer, reducer.name);
                assert_eq!(
                    outbox.signature_hash,
                    reducer_signature_hash(
                        &reducer.params(),
                        &reducer.ok_return_type,
                        &reducer.err_return_type,
                        validated.typespace(),
                    )
                );
                hashes.push(outbox.signature_hash);
                // Extracted schemas contain resolved remote-style metadata, so receiver
                // codegen and a second validation do not need the source reference.
                let exported: RawModuleDefV10 = validated.clone().into();
                assert!(exported.local_outboxes().is_none());
                let roundtrip: ModuleDef = exported.try_into().unwrap();
                assert_eq!(
                    roundtrip.tables().find_map(|table| table.outbox.as_ref()).unwrap(),
                    outbox
                );
            }
            assert_eq!(hashes[0], hashes[1]);
        }
    }
}

#[test]
fn local_outbox_signature_preserves_existing_wire_hash() {
    let mut builder = RawModuleDefV10Builder::new();
    builder.add_reducer("receivePing", ProductType::from([("Body", AlgebraicType::String)]));
    builder
        .build_table_with_new_type(
            "outboundPing",
            ProductType::from([
                ("id", AlgebraicType::U64),
                ("target", AlgebraicType::identity()),
                ("Body", AlgebraicType::String),
            ]),
            true,
        )
        .with_auto_inc_primary_key(0)
        .with_index_no_accessor_name(direct(0), "outbound_ping_id_idx")
        .finish();
    builder.add_local_outbox("outboundPing", "receivePing", 1, [2.into()], None::<&str>);
    let module: ModuleDef = builder.finish().try_into().unwrap();
    let outbox = module.tables().find_map(|table| table.outbox.as_ref()).unwrap();
    assert_eq!(outbox.remote_reducer.as_ref(), "receive_ping");
    assert_eq!(
        outbox.signature_hash.to_hex().to_string(),
        "386957a94632958d5ae8343e258f3a6f9dce9d7dafc46831050894baa7a4e4e1"
    );
}

#[test]
fn local_outbox_requires_a_reducer_source_name() {
    let mut raw = receiver_module(CaseConversionPolicy::SnakeCase, true, false);
    for section in &mut raw.sections {
        if let RawModuleDefV10Section::LocalOutboxes(outboxes) = section {
            // An explicit canonical name is not a valid source reference.
            outboxes[0].reducer_source_name = "WirePing".into();
        }
    }
    let errors = ModuleDef::try_from(raw).unwrap_err();
    assert!(
        errors.to_string().contains("local receiver reducer WirePing"),
        "{errors}"
    );
}

#[test]
fn local_and_remote_outboxes_cannot_share_a_table() {
    let mut raw = receiver_module(CaseConversionPolicy::None, false, false);
    raw.sections.push(RawModuleDefV10Section::Outboxes(vec![
        spacetimedb::spacetimedb_lib::db::raw_def::v10::RawOutboxDefV10 {
            table_name: "outboundPing".into(),
            remote_reducer: "remote_ping".into(),
            target_column: 1.into(),
            arg_columns: vec![2.into(), 3.into()],
            on_result_reducer: None,
            signature_hash: spacetimedb::spacetimedb_lib::Hash::ZERO,
        },
    ]));
    let errors = ModuleDef::try_from(raw).unwrap_err();
    assert!(errors.to_string().contains("assigned in multiple outboxes"), "{errors}");
}

#[test]
fn local_outbox_receiver_follows_its_submodule_namespace() {
    let child = receiver_module(CaseConversionPolicy::None, true, false);
    let mut middle = RawModuleDefV10Builder::new();
    middle.add_submodule("innerMount", child);
    let mut names = ExplicitNames::default();
    names.insert_namespace("innerMount", "InnerWire");
    middle.add_explicit_names(names);
    let mut root = RawModuleDefV10Builder::new();
    root.add_submodule("outerLib", middle.finish());
    let mut module: ModuleDef = root.finish().try_into().unwrap();

    for _ in 0..3 {
        let child = &module.submodules()["outer_lib"].submodules()["InnerWire"];
        let outbox = child.tables().find_map(|table| table.outbox.as_ref()).unwrap();
        assert_eq!(outbox.remote_reducer.as_ref(), "outer_lib.InnerWire.WirePing");
        let reducer = module.reducer_by_name(&outbox.remote_reducer).unwrap().1;
        assert_eq!(
            outbox.signature_hash,
            reducer_signature_hash(
                &reducer.params(),
                &reducer.ok_return_type,
                &reducer.err_return_type,
                child.typespace(),
            )
        );
        let raw: RawModuleDefV10 = module.into();
        module = raw.try_into().unwrap();
    }
}
