//! This script is used to generate the C++ bindings for the `RawModuleDef` type.
//! Run `cargo run --example regen-cpp-moduledef` to update C++ bindings whenever the module definition changes.
#![allow(clippy::disallowed_macros)]

use fs_err as fs;
use spacetimedb_codegen::{cpp, generate, CodegenOptions, OutputFile};
use spacetimedb_lib::db::raw_def::v10::{RawModuleDefV10, RawModuleDefV10Builder};
use spacetimedb_lib::RawModuleDef;
use spacetimedb_schema::def::ModuleDef;
use std::path::Path;

fn replace_required(code: String, from: &str, to: &str) -> anyhow::Result<String> {
    if !code.contains(from) {
        anyhow::bail!("expected generated C++ moduledef snippet was not found: {from:?}");
    }
    Ok(code.replace(from, to))
}

fn rewrite_raw_submodule_v10(code: String) -> anyhow::Result<String> {
    let code = replace_required(code, "#include \"RawModuleDefV10.g.h\"\n", "")?;
    let code = replace_required(
        code,
        "namespace SpacetimeDB::Internal {\n\nSPACETIMEDB_INTERNAL_PRODUCT_TYPE(RawSubmoduleV10)",
        "namespace SpacetimeDB::Internal {\nstruct RawModuleDefV10;\n} // namespace SpacetimeDB::Internal\n\nnamespace SpacetimeDB::Internal {\n\nSPACETIMEDB_INTERNAL_PRODUCT_TYPE(RawSubmoduleV10)",
    )?;
    let code = replace_required(
        code,
        "    SpacetimeDB::Internal::RawModuleDefV10 module;",
        "    std::shared_ptr<SpacetimeDB::Internal::RawModuleDefV10> module;",
    )?;
    replace_required(
        code,
        "    void bsatn_serialize(::SpacetimeDB::bsatn::Writer& writer) const {\n        ::SpacetimeDB::bsatn::serialize(writer, namespace_);\n        ::SpacetimeDB::bsatn::serialize(writer, module);\n    }\n    SPACETIMEDB_PRODUCT_TYPE_EQUALITY(namespace_, module)",
        "    void bsatn_serialize(::SpacetimeDB::bsatn::Writer& writer) const;\n    bool operator==(const RawSubmoduleV10& other) const;\n    bool operator!=(const RawSubmoduleV10& other) const;",
    )
}

fn append_raw_submodule_v10_definitions(code: String) -> anyhow::Result<String> {
    replace_required(
        code,
        "\n};\n} // namespace SpacetimeDB::Internal\n",
        "\n};\n\ninline void RawSubmoduleV10::bsatn_serialize(::SpacetimeDB::bsatn::Writer& writer) const {\n    ::SpacetimeDB::bsatn::serialize(writer, namespace_);\n    ::SpacetimeDB::bsatn::serialize(writer, *module);\n}\n\ninline bool RawSubmoduleV10::operator==(const RawSubmoduleV10& other) const {\n    return namespace_ == other.namespace_\n        && (module == other.module || (module && other.module && *module == *other.module));\n}\n\ninline bool RawSubmoduleV10::operator!=(const RawSubmoduleV10& other) const {\n    return !(*this == other);\n}\n} // namespace SpacetimeDB::Internal\n",
    )
}

fn main() -> anyhow::Result<()> {
    let mut builder = RawModuleDefV10Builder::new();
    builder.add_type::<RawModuleDef>();
    builder.add_type::<RawModuleDefV10>();
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
    .try_for_each(|OutputFile { filename, code }| -> anyhow::Result<()> {
        // Remove any prefix and just use the filename
        let filename = if let Some(name) = filename.strip_prefix("Types/") {
            name
        } else {
            &filename
        };

        let code = match filename {
            "RawSubmoduleV10.g.h" => rewrite_raw_submodule_v10(code)?,
            "RawModuleDefV10.g.h" => append_raw_submodule_v10_definitions(code)?,
            _ => code,
        };

        println!("Generating {}", filename);
        fs::write(dir.join(filename), code)?;
        Ok(())
    })?;

    println!("C++ autogen files written to: {}", dir.display());
    Ok(())
}
