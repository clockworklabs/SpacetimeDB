//! Run actual V8 hosts without a network or external service.
use super::*;
use crate::db::persistence::LocalPersistenceProvider;
use crate::host::module_host::CallProcedureParams;
use crate::host::{ArgsTuple, FunctionArgs};
use spacetimedb_lib::db::raw_def::{v10::FunctionVisibility, v10::RawModuleDefV10Builder, v9::Lifecycle};
use spacetimedb_paths::FromPathUnchecked;
use spacetimedb_primitives::ProcedureId;
use spacetimedb_sats::{AlgebraicType, ProductType};

fn program() -> Program {
    let mut schema = RawModuleDefV10Builder::new();
    schema.add_lifecycle_reducer(Lifecycle::Init, "init", ProductType::unit());
    schema.add_reducer("external", ProductType::unit());
    schema.add_reducer_with_visibility("internal", ProductType::unit(), Some(FunctionVisibility::Internal));
    schema.add_reducer_with_visibility("private", ProductType::unit(), Some(FunctionVisibility::Private));
    schema.add_procedure("external_procedure", ProductType::unit(), AlgebraicType::U8);
    schema.add_procedure_with_visibility(
        "internal_procedure",
        ProductType::unit(),
        AlgebraicType::U8,
        Some(FunctionVisibility::Internal),
    );
    let schema = spacetimedb_lib::bsatn::to_vec(&spacetimedb_lib::RawModuleDef::V10(schema.finish())).unwrap();
    Program::from_bytes(
        ModuleKind::JS,
        format!(
            r#"
            import {{ register_hooks }} from "spacetime:sys@1.0";
            import {{ register_hooks as register_procedures }} from "spacetime:sys@1.2";
            import {{ get_call_auth_flags }} from "spacetime:sys@2.2";
            register_hooks({{
                __describe_module__: function() {{ return new Uint8Array({schema:?}); }},
                __call_reducer__: function(id) {{
                    const expected = id === 0 ? 1 : 0;
                    if (get_call_auth_flags() !== expected) {{ throw new Error("incorrect invocation flags"); }}
                    return {{ tag: "ok" }};
                }},
            }});
            register_procedures({{ __call_procedure__: function() {{
                return new Uint8Array([get_call_auth_flags()]);
            }} }});
            "#
        )
        .into_bytes(),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invocation_flags_are_host_owned_and_internal_visibility_is_enforced() {
    let directory = tempfile::tempdir().unwrap();
    let data = Arc::new(ServerDataDir::from_path_unchecked(directory.path().to_owned()));
    let program = program();
    let initial = program.clone();
    let storage = move |hash| {
        let program = initial.clone();
        async move { Ok((program.hash == hash).then_some(program.bytes)) }
    };
    let controller = HostController::new(
        data.clone(),
        db::Config {
            storage: db::Storage::Memory,
            page_pool_max_size: None,
        },
        HostRuntimeConfig::default(),
        Arc::new(storage),
        Arc::new(NullEnergyMonitor),
        Arc::new(LocalPersistenceProvider::new(data)),
        JobCores::without_pinned_cores(),
    );
    let database = Database {
        id: 0xab10,
        database_identity: Identity::from_u256(0xab10u64.into()),
        owner_identity: Identity::ONE,
        host_type: HostType::Js,
        initial_program: program.hash,
    };
    // The init reducer itself asserts flags=1, so successful construction also
    // verifies the real host-to-JS syscall path for a trusted lifecycle call.
    let module = controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    for sender in [database.owner_identity, database.database_identity, Identity::ZERO] {
        module
            .call_reducer(sender, None, None, None, None, "external", FunctionArgs::Nullary)
            .await
            .unwrap()
            .outcome
            .into_result()
            .unwrap();
        for name in ["internal", "init"] {
            assert!(module
                .call_reducer(sender, None, None, None, None, name, FunctionArgs::Nullary)
                .await
                .is_err());
        }
        let result = module
            .call_procedure(sender, None, None, "external_procedure", FunctionArgs::Nullary)
            .await;
        assert_eq!(result.result.unwrap().return_val, AlgebraicValue::U8(0));
        assert!(module
            .call_procedure(sender, None, None, "internal_procedure", FunctionArgs::Nullary)
            .await
            .result
            .is_err());
        assert_eq!(
            module
                .call_reducer(sender, None, None, None, None, "private", FunctionArgs::Nullary)
                .await
                .is_ok(),
            sender == database.owner_identity,
        );
    }
    // Trusted host work uses its explicit constructor. An ordinary later call
    // observes zero again even if the procedure instance is reused.
    let result = module
        .call_procedure_with_params(
            "internal_procedure",
            CallProcedureParams::from_system(
                Timestamp::now(),
                database.database_identity,
                ProcedureId(1),
                ArgsTuple::nullary(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(result.result.unwrap().return_val, AlgebraicValue::U8(1));
    let result = module
        .call_procedure(Identity::ONE, None, None, "external_procedure", FunctionArgs::Nullary)
        .await;
    assert_eq!(result.result.unwrap().return_val, AlgebraicValue::U8(0));
    drop(module);
    controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap();
}
