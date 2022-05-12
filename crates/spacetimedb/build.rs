extern crate glob;
extern crate prost_build;
use glob::glob;
use std::fs;
use std::path::PathBuf;

fn main() {
    // Generate BTreeMap fields for all messages. This forces encoded output to be consistent, so
    // that encode/decode roundtrips can use encoded output for comparison. Otherwise trying to
    // compare based on the Rust PartialEq implementations is difficult, due to presence of NaN
    // values.
    let src = PathBuf::from("protobuf");
    let includes = &[src.clone()];

    let mut config = prost_build::Config::new();
    config.btree_map(&["."]);

    let out_dir = &PathBuf::from("src/messages");
    fs::create_dir_all(out_dir).expect("failed to create prefix directory");
    config.out_dir(out_dir);

    let mut path_bufs: Vec<PathBuf> = Vec::new();
    for e in glob("protobuf/*.proto").expect("Failed to read glob pattern") {
        path_bufs.push(e.unwrap().clone());
    }
    config.compile_protos(&path_bufs[..], includes).unwrap();
}
