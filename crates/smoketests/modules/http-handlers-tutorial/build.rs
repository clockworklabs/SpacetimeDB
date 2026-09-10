use std::{env, fs, path::PathBuf};

fn main() {
    let doc_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md");
    println!("cargo:rerun-if-changed={}", doc_path.display());
    let doc = fs::read_to_string(&doc_path).expect("Failed to read HTTP handlers tutorial");
    let doc = doc.replace("\r\n", "\n");
    let blocks: Vec<_> = doc
        .split("```rust\n")
        .skip(1)
        .map(|block| {
            block
                .split_once("\n```")
                .expect("Unterminated Rust code block in HTTP handlers tutorial")
                .0
        })
        .collect();
    assert!(
        !blocks.is_empty(),
        "No Rust code blocks found in HTTP handlers tutorial"
    );
    let out_path = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("module.rs");
    fs::write(out_path, blocks.join("\n\n")).expect("Failed to write HTTP handlers tutorial module");
}
