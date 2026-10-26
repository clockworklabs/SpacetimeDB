use spacetimedb::spacetimedb_lib::db::raw_def::v10::{
    CaseConversionPolicy, ExplicitNames, RawModuleDefV10, RawModuleDefV10Builder, RawModuleDefV10Section,
};
use spacetimedb::spacetimedb_lib::db::reducer_signature::raw_reducer_signature;
use spacetimedb::spacetimedb_lib::{bsatn, AlgebraicType, ProductType};
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
fn raw_receiver_signatures_match_host_validation() {
    for policy in [CaseConversionPolicy::None, CaseConversionPolicy::SnakeCase] {
        for explicit_name in [false, true] {
            let mut hashes = Vec::new();
            for unrelated_type in [false, true] {
                let raw = receiver_module(policy, explicit_name, unrelated_type);
                let before = bsatn::to_vec(&raw).unwrap();
                let (name, hash) = raw_reducer_signature(&raw, "receivePing").unwrap();
                assert_eq!(before, bsatn::to_vec(&raw).unwrap());
                let validated: ModuleDef = raw.try_into().unwrap();
                let reducer = validated
                    .reducers()
                    .find(|reducer| reducer.accessor_name.as_ref() == "receivePing")
                    .unwrap();
                assert_eq!(name.as_ref(), reducer.name.as_ref());
                assert_eq!(
                    hash,
                    reducer_signature_hash(
                        &reducer.params(),
                        &reducer.ok_return_type,
                        &reducer.err_return_type,
                        validated.typespace(),
                    )
                );
                hashes.push(hash);
            }
            assert_eq!(hashes[0], hashes[1]);
        }
    }
}

#[test]
fn raw_receiver_signature_preserves_existing_wire_hash() {
    let mut builder = RawModuleDefV10Builder::new();
    builder.add_reducer("receivePing", ProductType::from([("Body", AlgebraicType::String)]));
    let raw = builder.finish();
    let (name, hash) = raw_reducer_signature(&raw, "receivePing").unwrap();
    assert_eq!(name.as_ref(), "receive_ping");
    assert_eq!(
        hash.to_hex().to_string(),
        "386957a94632958d5ae8343e258f3a6f9dce9d7dafc46831050894baa7a4e4e1"
    );
    assert!(raw_reducer_signature(&raw, "missing").is_none());
}
