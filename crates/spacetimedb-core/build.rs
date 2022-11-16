use std::fs;

fn main() {
    // Generate BTreeMap fields for all messages. This forces encoded output to be consistent, so
    // that encode/decode roundtrips can use encoded output for comparison. Otherwise trying to
    // compare based on the Rust PartialEq implementations is difficult, due to presence of NaN
    // values.

    let proto_dir = "protobuf";
    println!("cargo:rerun-if-changed={proto_dir}");

    let protos = fs::read_dir(proto_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension() == Some("proto".as_ref()))
        .collect::<Vec<_>>();
    let includes = &[proto_dir];

    prost_build::Config::new()
        .btree_map(["."])
        .include_file("protobuf.rs")
        .type_attribute(
            ".control_db.HostType",
            r#"#[derive(strum::EnumString, strum::AsRefStr)] #[strum(serialize_all = "lowercase")]"#,
        )
        .compile_protos(&protos, includes)
        .unwrap();
}
