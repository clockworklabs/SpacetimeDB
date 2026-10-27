use spacetimedb_codegen::{generate_receiver_bindings, ReceiverLanguage};
use spacetimedb_lib::db::raw_def::v10::RawModuleDefV10Builder;
use spacetimedb_lib::{AlgebraicType, ProductType};
use spacetimedb_schema::def::ModuleDef;
use std::collections::BTreeMap;
use std::process::Command;

// Keep the snapshot focused on public types and reducer argument associations.
// The generated macro bodies are exercised by compiling the complete bindings.
fn type_declarations(code: &str) -> String {
    let mut in_item = false;
    code.lines()
        .filter(|line| {
            if line.starts_with("pub use ")
                || line.starts_with("pub struct ")
                || line.starts_with("impl spacetimedb::rt::RemoteReducer for ")
            {
                in_item = line.ends_with('{');
                true
            } else if in_item {
                if line.trim() == "}" {
                    in_item = false;
                }
                true
            } else {
                false
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn receiver_bindings_and_outbox_macros_preserve_builtin_and_custom_types() {
    let mut builder = RawModuleDefV10Builder::new();
    // Receiver-defined names can collide with both builtins and the synthetic
    // argument product generated for `receive`.
    let custom_string = builder.add_algebraic_type(
        [],
        "String",
        AlgebraicType::product([("value", AlgebraicType::U64)]),
        false,
    );
    let custom_receive_args = builder.add_algebraic_type(
        [],
        "ReceiveArgs",
        AlgebraicType::product([
            ("builtin", AlgebraicType::String),
            ("custom", AlgebraicType::Ref(custom_string)),
        ]),
        false,
    );
    builder.add_reducer(
        "receive",
        ProductType::from([
            ("text", AlgebraicType::String),
            ("values", AlgebraicType::array(AlgebraicType::U64)),
            ("maybe", AlgebraicType::option(AlgebraicType::String)),
            (
                "outcome",
                AlgebraicType::result(AlgebraicType::unit(), AlgebraicType::String),
            ),
            ("number", AlgebraicType::U64),
            ("timestamp", AlgebraicType::timestamp()),
            ("identity", AlgebraicType::identity()),
            ("custom_string", AlgebraicType::Ref(custom_string)),
            ("custom_args", AlgebraicType::Ref(custom_receive_args)),
        ]),
    );
    builder.add_reducer("noop", ProductType::unit());
    let module: ModuleDef = builder.finish().try_into().unwrap();
    let files = generate_receiver_bindings(&module, ReceiverLanguage::Rust, None).unwrap();
    let declarations = files
        .iter()
        .map(|file| (file.filename.as_str(), type_declarations(&file.code)))
        .collect::<BTreeMap<_, _>>();
    insta::assert_toml_snapshot!("receiver_type_declarations", declarations);
    let project = tempfile::tempdir().unwrap();
    let receiver_dir = project.path().join("src/receiver");
    std::fs::create_dir_all(&receiver_dir).unwrap();
    for file in files {
        let name = if file.filename == "lib.rs" {
            "mod.rs"
        } else {
            &file.filename
        };
        std::fs::write(receiver_dir.join(name), file.code).unwrap();
    }
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let bindings_path = serde_json::to_string(&repo.join("crates/bindings")).unwrap();
    std::fs::write(
        project.path().join("Cargo.toml"),
        format!(
            r#"[package]
name = "outbox-builtin-shadowing"
version = "0.0.0"
edition = "2024"

[workspace]

[dependencies]
spacetimedb = {{ path = {bindings_path} }}
"#
        ),
    )
    .unwrap();

    let shadows = r#"
    struct String;
    struct Vec<T>(::core::marker::PhantomData<T>);
    struct Option<T>(::core::marker::PhantomData<T>);
    struct Result<T, E>(::core::marker::PhantomData<(T, E)>);
    type u64 = ();
    mod core {}
    mod std {}
    mod spacetimedb {}
"#;
    // First compile the complete generated bindings in an ordinary sender, then
    // expand the same receiver macro with builtin names shadowed at its call site.
    for (scope, shadowing) in [("unshadowed", ""), ("shadowed", shadows)] {
        let source = format!(
            r#"
#![allow(dead_code, non_camel_case_types, unused_imports)]
mod receiver;
mod caller {{
    {shadowing}
    fn check() {{
        let _ = crate::receiver::__spacetimedb_typecheck_outbox_receive!(crate::receiver, {{
            (0, text: ::std::string::String as text),
            (1, values: ::std::vec::Vec<::core::primitive::u64> as values),
            (2, maybe: ::core::option::Option<::std::string::String> as maybe),
            (3, outcome: ::core::result::Result<(), ::std::string::String> as outcome),
            (4, number: ::core::primitive::u64 as number),
            (5, timestamp: ::spacetimedb::Timestamp as timestamp),
            (6, identity: crate::receiver::Identity as identity),
            (7, custom_string: crate::receiver::String as custom_string),
            (8, custom_args: crate::receiver::ReceiveArgs as custom_args),
        }});
        let _ = |args: <crate::receiver::receive as ::spacetimedb::rt::RemoteReducer>::Args| {{
            let _: ::std::string::String = args.text;
            let _: crate::receiver::String = args.custom_string;
            let custom_args: crate::receiver::ReceiveArgs = args.custom_args;
            let _: ::std::string::String = custom_args.builtin;
            let _: crate::receiver::String = custom_args.custom;
        }};
    }}
}}
"#
        );
        std::fs::write(project.path().join("src/lib.rs"), source).unwrap();
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| repo.join("target"));
        let output = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args(["check", "--offline", "--manifest-path"])
            .arg(project.path().join("Cargo.toml"))
            .arg("--target-dir")
            .arg(target)
            .output()
            .expect("cargo is required to check generated receiver bindings");
        assert!(
            output.status.success(),
            "generated receiver bindings failed in the {scope} sender:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
