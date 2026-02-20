//! This script is used to generate the C++ bindings for the `RawModuleDef` type.
//! Run `cargo run --example regen-cpp-moduledef` to update C++ bindings whenever the module definition changes.
#![allow(clippy::disallowed_macros)]

use fs_err as fs;
use spacetimedb_codegen::{cpp, generate, CodegenOptions, OutputFile};
use spacetimedb_lib::db::raw_def::v9::{RawModuleDefV9, RawModuleDefV9Builder};
use spacetimedb_schema::def::ModuleDef;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let mut builder = RawModuleDefV9Builder::new();
    builder.add_type::<RawModuleDefV9>();
    let module = builder.finish();

    // Build relative path from the codegen crate to the C++ Module Library autogen directory
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let dir = Path::new(manifest_dir)
        .parent()
        .unwrap()
        .join("bindings-cpp/include/spacetimedb/internal/autogen");

    println!("Target directory path: {}", dir.display());

    // Create the autogen directory if it doesn't exist
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;

    let module: ModuleDef = module.try_into()?;
    generate(
        &module,
        &cpp::Cpp {
            namespace: "SpacetimeDB::Internal",
        },
        &CodegenOptions::default(),
    )
    .into_iter()
    .try_for_each(|OutputFile { filename, code }| {
        // Remove any prefix and just use the filename
        let filename = if let Some(name) = filename.strip_prefix("Types/") {
            name
        } else {
            &filename
        };

        println!("Generating {}", filename);
        fs::write(dir.join(filename), code)
    })?;

    println!("C++ autogen files written to: {}", dir.display());
    Ok(())
}
