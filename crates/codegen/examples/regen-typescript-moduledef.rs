//! This script is used to generate the C# bindings for the `RawModuleDef` type.
//! Run `cargo run --example regen-csharp-moduledef` to update C# bindings whenever the module definition changes.

use fs_err as fs;
use regex::Regex;
use spacetimedb_codegen::{generate, typescript, OutputFile};
use spacetimedb_lib::{RawModuleDef, RawModuleDefV8};
use spacetimedb_schema::def::ModuleDef;
use std::path::Path;
use std::sync::OnceLock;

macro_rules! regex_replace {
    ($value:expr, $re:expr, $replace:expr) => {{
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new($re).unwrap())
            .replace_all($value, $replace)
    }};
}

fn main() -> anyhow::Result<()> {
    let module = RawModuleDefV8::with_builder(|module| {
        module.add_type::<RawModuleDef>();
    });

    let dir = &Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../bindings-typescript/src/autogen"
    ))
    .canonicalize()?;

    fs::remove_dir_all(dir)?;
    fs::create_dir(dir)?;

    let module: ModuleDef = module.try_into()?;
    generate(&module, &typescript::TypeScript)
        .into_iter()
        .try_for_each(|OutputFile { filename, code }| {
            // Skip the index.ts since we don't need it.
            if filename == "index.ts" {
                return Ok(());
            }
            let code = regex_replace!(&code, r"@clockworklabs/spacetimedb-sdk", "../index");
            fs::write(dir.join(filename), code.as_bytes())
        })?;

    Ok(())
}
