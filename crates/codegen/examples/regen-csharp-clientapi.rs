//! Generate the C# SDK's own wire protocol types, independently of client bindings.
use spacetimedb_codegen::csharp::generate_runtime_type;
use spacetimedb_lib::de::serde::DeserializeWrapper;
use spacetimedb_lib::RawModuleDef;
use spacetimedb_schema::def::ModuleDef;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let DeserializeWrapper(raw): DeserializeWrapper<RawModuleDef> = serde_json::from_reader(std::io::stdin().lock())?;
    let module = ModuleDef::try_from(raw)?;
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdks/csharp/src/SpacetimeDB/ClientApi");
    for ty in module.types() {
        let file = generate_runtime_type(&module, ty, "SpacetimeDB.ClientApi");
        std::fs::write(out.join(file.filename), file.code)?;
    }
    Ok(())
}
