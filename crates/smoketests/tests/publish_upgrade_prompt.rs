use std::path::PathBuf;

use spacetimedb_smoketests::{random_string, workspace_root, Smoketest};

const MODULE_CODE: &str = r#"
use spacetimedb::{reducer, ReducerContext};

#[reducer]
pub fn noop(_ctx: &ReducerContext) {}
"#;

fn old_fixture_wasm() -> PathBuf {
    workspace_root()
        .join("crates")
        .join("smoketests")
        .join("fixtures")
        .join("upgrade_old_module_v1.wasm")
}

#[test]
fn upgrade_prompt_on_publish() {
    let mut test = Smoketest::builder().autopublish(false).build();

    let old_wasm = old_fixture_wasm();
    assert!(old_wasm.exists(), "expected old fixture wasm at {}", old_wasm.display());

    let db_name = format!("upgrade-smoke-{}", random_string());

    test.use_precompiled_wasm_path(&old_wasm).unwrap();
    let initial_identity = test.publish_module_named(&db_name, false).unwrap();
    assert_eq!(test.database_identity.as_deref(), Some(initial_identity.as_str()));

    // Switch back to source-built module, which uses current bindings.
    test.write_module_code(MODULE_CODE).unwrap();

    let deny_err = test.publish_module_named(&db_name, false).unwrap_err().to_string();
    assert!(deny_err.contains("major version upgrade from 1.0 to 2.0"));
    assert!(deny_err.contains("Please type 'upgrade' to accept this change:"));

    let accepted_identity = test.publish_module_with_stdin(&db_name, "upgrade\n").unwrap();
    assert_eq!(accepted_identity, initial_identity);
}
