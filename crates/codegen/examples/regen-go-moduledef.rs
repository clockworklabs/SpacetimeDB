//! This script is used to generate the Go bindings for the `RawModuleDef` type.
//! Run `cargo run --example regen-go-moduledef` to update Go bindings whenever the module definition changes.

use fs_err as fs;
use spacetimedb_codegen::{go, generate};
use spacetimedb_lib::{RawModuleDef, RawModuleDefV8};
use spacetimedb_schema::def::ModuleDef;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let module = RawModuleDefV8::with_builder(|module| {
        module.add_type::<RawModuleDef>();
    });

    let dir = &Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../bindings-go/internal/autogen"
    ))
    .canonicalize()?;

    fs::remove_dir_all(dir)?;
    fs::create_dir(dir)?;

    let module: ModuleDef = module.try_into()?;
    generate(
        &module,
        &go::Go {
            package_name: "autogen".to_string(),
        },
    )
    .into_iter()
    .try_for_each(|(filename, code)| {
        // Convert filename to Go conventions
        let filename = filename.replace('_', "_").to_lowercase();
        let filename = if !filename.ends_with(".go") {
            format!("{}.go", filename.strip_suffix(".rs").unwrap_or(&filename))
        } else {
            filename
        };

        // The Go codegen already generates correct Go code, no transformations needed
        fs::write(dir.join(filename), &code)
    })?;

    Ok(())
} 