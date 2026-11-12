use spacetimedb_codegen::{generate_receiver_bindings, ReceiverLanguage};
use spacetimedb_lib::db::raw_def::v10::{CaseConversionPolicy, ExplicitNames, RawModuleDefV10Builder};
use spacetimedb_lib::db::raw_def::v9::Lifecycle;
use spacetimedb_lib::{AlgebraicType, ProductType};
use spacetimedb_schema::def::ModuleDef;

/// A receiver like the IDC smoketest receiver: nested custom types, a sum type with
/// payloads, a plain enum, and a reducer whose canonical name differs from its accessor.
fn receiver_module() -> ModuleDef {
    let mut builder = RawModuleDefV10Builder::new();
    builder.set_case_conversion_policy(CaseConversionPolicy::SnakeCase);
    let state = builder.add_algebraic_type(
        [],
        "PingState",
        AlgebraicType::sum([
            ("Ready", AlgebraicType::unit()),
            ("Deferred", AlgebraicType::option(AlgebraicType::String)),
        ]),
        true,
    );
    let mood = builder.add_algebraic_type(
        [],
        "Mood",
        AlgebraicType::simple_enum(["Calm", "Excited"].into_iter()),
        true,
    );
    let checkpoint = builder.add_algebraic_type(
        [],
        "PingCheckpoint",
        AlgebraicType::product([("sequence", AlgebraicType::U32), ("state", AlgebraicType::Ref(state))]),
        true,
    );
    let details = builder.add_algebraic_type(
        [],
        "PingDetails",
        AlgebraicType::product([
            ("labels", AlgebraicType::array(AlgebraicType::String)),
            ("checkpoints", AlgebraicType::array(AlgebraicType::Ref(checkpoint))),
            ("note", AlgebraicType::option(AlgebraicType::String)),
            ("mood", AlgebraicType::Ref(mood)),
        ]),
        true,
    );
    builder.add_algebraic_type(
        [],
        "Unrelated",
        AlgebraicType::product([("id", AlgebraicType::U64)]),
        true,
    );
    builder.add_reducer("receivePing", ProductType::from([("Body", AlgebraicType::String)]));
    builder.add_reducer(
        "receive_detailed_ping",
        ProductType::from([
            ("body", AlgebraicType::String),
            ("details", AlgebraicType::Ref(details)),
        ]),
    );
    builder.add_reducer("Noop", ProductType::unit());
    builder.add_lifecycle_reducer(Lifecycle::Init, "initialize", ProductType::unit());
    let mut names = ExplicitNames::default();
    names.insert_function("receivePing", "wire_ping");
    builder.add_explicit_names(names);
    builder.finish().try_into().unwrap()
}

fn header(module: &ModuleDef, dependency: Option<&str>) -> String {
    let files = generate_receiver_bindings(module, ReceiverLanguage::Cpp, dependency).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].filename, "bindings.h");
    files.into_iter().next().unwrap().code
}

fn signature_hash(module: &ModuleDef, reducer: &str) -> String {
    let reducer = module.reducers().find(|r| r.name.as_ref() == reducer).unwrap();
    spacetimedb_schema::def::reducer_signature_hash(
        &reducer.params(),
        &reducer.ok_return_type,
        &reducer.err_return_type,
        module.typespace(),
    )
    .to_hex()
    .to_string()
}

#[test]
fn cpp_receiver_bindings() {
    let module = receiver_module();
    let code = header(&module, None);

    assert!(code.starts_with(spacetimedb_codegen::AUTO_GENERATED_PREFIX));
    assert!(code.contains("namespace remote_module {"));
    assert!(code.contains("using Identity = ::SpacetimeDB::RemoteIdentity<detail::module_tag>;"));
    // Handles use the accessor name and carry the canonical name.
    assert!(code.contains("struct receive_ping {"));
    assert!(code.contains("static constexpr std::string_view name = \"wire_ping\";"));
    assert!(code.contains("using param_types = std::tuple<std::string>;"));
    assert!(code.contains("param_names{{\"body\"}}"));
    assert!(code.contains("struct noop {"));
    // Each handle names the row type generated for SPACETIMEDB_OUTBOX_FOR.
    assert!(code.contains("using outbox_row = ::remote_module::outbox::receive_ping;"));
    assert!(code.contains(
        "struct receive_detailed_ping {\n    uint64_t msg_id;\n    ::remote_module::Identity target;\n    std::string body;\n    ::remote_module::PingDetails details;\n};"
    ));
    assert!(code.contains("}, \"ReceivePingOutbox\");"));
    assert!(code
        .contains("SPACETIMEDB_GENERATE_FIELD_REGISTRAR_WITH_FIELDS(::remote_module::outbox::noop, msg_id, target)"));
    assert!(code.contains("struct ::SpacetimeDB::is_outbox_row<::remote_module::outbox::noop> : std::true_type {};"));
    assert!(code.contains("static constexpr std::array<std::string_view, 0> param_names{{}};"));
    for reducer in ["wire_ping", "receive_detailed_ping", "noop"] {
        assert!(code.contains(&format!(
            "static constexpr std::string_view signature_hash = \"{}\";",
            signature_hash(&module, reducer)
        )));
    }
    // Lifecycle reducers can't be called, and only reachable types are emitted.
    assert!(!code.contains("initialize"));
    assert!(!code.contains("Unrelated"));
    // Without a dependency name, types register under their own names.
    assert!(code.contains("}, \"PingDetails\");"));
    assert!(code.contains("getOrRegister("));
    assert!(!code.contains("getOrRegisterExactScope"));
    // Types held by value are defined before the types that hold them.
    let state = code.find("struct PingState {").unwrap();
    let checkpoint = code.find("struct PingCheckpoint {").unwrap();
    let details = code.find("struct PingDetails {").unwrap();
    let mood = code.find("enum class Mood : uint8_t {").unwrap();
    assert!(state < checkpoint && mood < details);

    insta::assert_snapshot!(code);
}

#[test]
fn cpp_receiver_types_are_scoped_by_dependency_name() {
    let module = receiver_module();
    for dependency in ["game_world", "auth"] {
        let code = header(&module, Some(dependency));
        assert!(code.contains(&format!("namespace {dependency} {{")));
        assert!(code.contains(&format!("namespace {dependency}::reducers {{")));
        for name in ["PingDetails", "PingCheckpoint", "PingState", "Mood"] {
            assert!(code.contains(&format!("}}, \"{dependency}.{name}\");")));
            assert!(!code.contains(&format!("}}, \"{name}\");")));
        }
        // Scoped types must not merge with a sender's own type of the same name.
        assert!(code.contains("getOrRegisterExactScope("));
        assert!(code.contains(&format!("using identity_type = ::{dependency}::Identity;")));
        assert!(code.contains(&format!("}}, \"{dependency}.ReceivePingOutbox\");")));
    }
}

#[test]
fn cpp_receiver_names_avoid_keywords_and_collisions() {
    let mut builder = RawModuleDefV10Builder::new();
    // Collides with the bindings' own `Identity`.
    let identity = builder.add_algebraic_type(
        [],
        "Identity",
        AlgebraicType::product([("class", AlgebraicType::U8), ("value", AlgebraicType::String)]),
        true,
    );
    let tagged = builder.add_algebraic_type(
        [],
        "Tagged",
        AlgebraicType::sum([
            ("Tag", AlgebraicType::U8),
            ("Value", AlgebraicType::U8),
            ("Other", AlgebraicType::U8),
        ]),
        true,
    );
    builder.add_reducer(
        "delete",
        ProductType::from([
            ("namespace", AlgebraicType::Ref(identity)),
            ("tagged", AlgebraicType::Ref(tagged)),
        ]),
    );
    let module: ModuleDef = builder.finish().try_into().unwrap();
    let code = header(&module, Some("namespace"));

    assert!(code.contains("namespace namespace_ {"));
    assert!(code.contains("struct Identity_ {"));
    assert!(code.contains("uint8_t class_;"));
    assert!(code.contains("std::string value;"));
    assert!(code.contains("struct delete_ {"));
    assert!(code.contains("static constexpr std::string_view name = \"delete\";"));
    assert!(code.contains("param_names{{\"namespace\", \"tagged\"}}"));
    // Sum variants can't shadow the members declared next to them.
    assert!(code.contains("Tag_ = 0,"));
    assert!(code.contains("static Tagged Tag_(uint8_t payload)"));
    assert!(code.contains("static Tagged Value(uint8_t payload)"));
    // The registered type keeps the receiver's name.
    assert!(code.contains("}, \"namespace.Identity\");"));
    // `namespace` can't be a C++ column name, so `delete` has no generated outbox row.
    assert!(!code.contains("using outbox_row"));
}

#[test]
fn cpp_receiver_bindings_cover_builtin_types() {
    let mut builder = RawModuleDefV10Builder::new();
    builder.add_reducer(
        "receive",
        ProductType::from([
            ("flag", AlgebraicType::Bool),
            ("small", AlgebraicType::I8),
            ("byte", AlgebraicType::U8),
            ("short_int", AlgebraicType::I16),
            ("ushort_int", AlgebraicType::U16),
            ("int_value", AlgebraicType::I32),
            ("uint_value", AlgebraicType::U32),
            ("long_int", AlgebraicType::I64),
            ("ulong_int", AlgebraicType::U64),
            ("wide", AlgebraicType::I128),
            ("uwide", AlgebraicType::U128),
            ("huge", AlgebraicType::I256),
            ("uhuge", AlgebraicType::U256),
            ("single", AlgebraicType::F32),
            ("double_value", AlgebraicType::F64),
            ("text", AlgebraicType::String),
            ("bytes", AlgebraicType::bytes()),
            ("maybe", AlgebraicType::option(AlgebraicType::String)),
            (
                "outcome",
                AlgebraicType::result(AlgebraicType::unit(), AlgebraicType::String),
            ),
            ("identity", AlgebraicType::identity()),
            ("connection_id", AlgebraicType::connection_id()),
            ("timestamp", AlgebraicType::timestamp()),
            ("duration", AlgebraicType::time_duration()),
            ("uuid", AlgebraicType::uuid()),
        ]),
    );
    let module: ModuleDef = builder.finish().try_into().unwrap();
    let code = header(&module, None);
    assert!(code.contains(
        "using param_types = std::tuple<bool, int8_t, uint8_t, int16_t, uint16_t, int32_t, uint32_t, int64_t, \
         uint64_t, ::SpacetimeDB::I128, ::SpacetimeDB::U128, ::SpacetimeDB::I256, ::SpacetimeDB::U256, float, \
         double, std::string, std::vector<uint8_t>, std::optional<std::string>, \
         ::SpacetimeDB::Result<std::monostate, std::string>, ::SpacetimeDB::Identity, ::SpacetimeDB::ConnectionId, \
         ::SpacetimeDB::Timestamp, ::SpacetimeDB::TimeDuration, ::SpacetimeDB::Uuid>;"
    ));
    // A plain `Identity` parameter is not a target identity.
    assert!(!code.contains("RemoteIdentity<detail::module_tag>>"));
}

/// The C++ compile tests build modules against these checked-in headers.
#[test]
fn cpp_receiver_fixtures_are_up_to_date() {
    let module = receiver_module();
    for dependency in ["receiver", "other"] {
        let path = format!(
            "{}/../bindings-cpp/tests/fixtures/idc_receiver/{dependency}/bindings.h",
            env!("CARGO_MANIFEST_DIR")
        );
        let expected = header(&module, Some(dependency));
        if std::env::var_os("SPACETIMEDB_UPDATE_FIXTURES").is_some() {
            fs_err::create_dir_all(std::path::Path::new(&path).parent().unwrap()).unwrap();
            fs_err::write(&path, &expected).unwrap();
        }
        let actual = fs_err::read_to_string(&path).unwrap_or_default();
        assert!(
            actual == expected,
            "{path} is out of date; rerun with SPACETIMEDB_UPDATE_FIXTURES=1"
        );
    }
}

#[test]
fn cpp_receiver_outbox_rows_avoid_metadata_columns_and_type_names() {
    let mut builder = RawModuleDefV10Builder::new();
    // Same name as the outbox row generated for `receive`.
    let collides = builder.add_algebraic_type(
        [],
        "ReceiveOutbox",
        AlgebraicType::product([("value", AlgebraicType::U8)]),
        true,
    );
    builder.add_reducer("receive", ProductType::from([("value", AlgebraicType::Ref(collides))]));
    builder.add_reducer("with_target", ProductType::from([("target", AlgebraicType::String)]));
    builder.add_reducer("with_msg_id", ProductType::from([("msg_id", AlgebraicType::U64)]));
    let module: ModuleDef = builder.finish().try_into().unwrap();
    let code = header(&module, Some("game_world"));

    assert!(code.contains("}, \"game_world.ReceiveOutbox\");"));
    assert!(code.contains("}, \"game_world.ReceiveOutbox_\");"));
    assert!(code.contains("using outbox_row = ::game_world::outbox::receive;"));
    // The shorthand's own `msg_id` and `target` columns would collide with these parameters.
    assert!(!code.contains("outbox::with_target"));
    assert!(!code.contains("outbox::with_msg_id"));
}
