//! This script is used to generate the Typescript bindings for the `RawModuleDef` type.
//! Run `cargo run --example regen-typescript-moduledef` to update TS bindings whenever the module definition changes.

// TODO: consider renaming this file, since it doesn't just generate `RawModuleDef` anymore.

use fs_err as fs;
use regex::Regex;
use spacetimedb_codegen::{generate, typescript, OutputFile};
use spacetimedb_lib::db::raw_def::v9::ViewResultHeader;
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
        module.add_type::<ViewResultHeader>();
        module.add_type::<spacetimedb_lib::http::Request>();
        module.add_type::<spacetimedb_lib::http::Response>();
    });

    let dir = &Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../bindings-typescript/src/lib/autogen"
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
            let code = regex_replace!(&code, r#"from "spacetimedb";"#, r#"from "../../lib/type_builders";"#);

            // Elide types which are related to client-side only things
            let code = regex_replace!(&code, r"type CallReducerFlags as __CallReducerFlags,", r"");
            let code = regex_replace!(&code, r"type ErrorContextInterface as __ErrorContextInterface,", r"");
            let code = regex_replace!(&code, r"type Event as __Event,", r"");
            let code = regex_replace!(&code, r"type EventContextInterface as __EventContextInterface,", r"");
            let code = regex_replace!(
                &code,
                r"type ReducerEventContextInterface as __ReducerEventContextInterface,",
                r""
            );
            let code = regex_replace!(
                &code,
                r"type SubscriptionEventContextInterface as __SubscriptionEventContextInterface,",
                r""
            );
            let code = regex_replace!(&code, r"DbConnectionBuilder as __DbConnectionBuilder,", r"");
            let code = regex_replace!(&code, r"DbConnectionImpl as __DbConnectionImpl,", r"");
            let code = regex_replace!(&code, r"type DbConnectionConfig as __DbConnectionConfig,", r"");
            let code = regex_replace!(&code, r"SubscriptionBuilderImpl as __SubscriptionBuilderImpl,", r"");
            let code = regex_replace!(&code, r"TableCache as __TableCache,", r"");
            let code = regex_replace!(&code, r"ClientCache as __ClientCache,", r"");
            fs::write(dir.join(filename), code.as_bytes())
        })?;

    Ok(())
}
