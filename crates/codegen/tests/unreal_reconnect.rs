use spacetimedb_codegen::{generate, CodegenOptions, UnrealCpp};
use spacetimedb_lib::{RawModuleDef, RawModuleDefV8};
use spacetimedb_schema::def::ModuleDef;

// Exercise both prefixed and unprefixed bindings: lifecycle delegates must use
// the concrete connection type while token requests remain SDK-wide types.
#[test]
fn unreal_reconnect_builder_and_callback_forwarding() {
    let module = ModuleDef::try_from(RawModuleDef::V8BackCompat(RawModuleDefV8::with_builder(|_| {}))).unwrap();
    let root = std::env::temp_dir().join(format!("spacetimedb-unreal-reconnect-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    std::fs::write(
        root.join("Game.uproject"),
        r#"{"Modules":[{"Name":"Game","Type":"Runtime"}]}"#,
    )
    .unwrap();
    for prefix in ["", "Game"] {
        let files = generate(
            &module,
            &UnrealCpp {
                module_name: "Game",
                module_prefix: prefix,
                uproject_dir: &root,
            },
            &CodegenOptions::default(),
        );
        let header = &files
            .iter()
            .find(|f| f.filename.ends_with("SpacetimeDBClient.g.h"))
            .unwrap()
            .code;
        let source = &files
            .iter()
            .find(|f| f.filename.ends_with("SpacetimeDBClient.g.cpp"))
            .unwrap()
            .code;
        for signature in [
            format!("U{prefix}DbConnectionBuilder* WithAutomaticReconnect();"),
            format!("U{prefix}DbConnectionBuilder* WithAutomaticReconnectOptions(const FAutomaticReconnectOptions& Options);"),
            format!("U{prefix}DbConnectionBuilder* WithTokenProvider(FTokenProviderDelegate Provider);"),
            format!("U{prefix}DbConnectionBuilder* OnAutomaticReconnect(F{prefix}OnConnectDelegate Callback);"),
            format!("static bool IsReconnecting(const F{prefix}ContextBase& Ctx)\n\t{{\n\t\treturn Ctx.IsReconnecting();\n\t}}"),
        ] {
            assert!(header.contains(&signature), "missing API: {signature}");
        }
        assert!(header.contains("bool bWillReconnect, int32 NextReconnectAttempt, float NextReconnectDelaySeconds"));
        assert!(source.contains("OnAutomaticReconnectDelegate.ExecuteIfBound(this, InIdentity, InToken)"));
        assert!(source.contains("OnDisconnectDelegate.Execute(this, Error, bWillReconnect, NextReconnectAttempt, NextReconnectDelaySeconds)"));
        assert!(source.contains("OnAutomaticReconnectBase(BaseReconnect)"));
        assert!(source.contains("ReducerEvent.CallError          = Event.CallError"));
        assert!(source.contains("ProcedureEvent.CallError          = Event.CallError"));
    }
}
