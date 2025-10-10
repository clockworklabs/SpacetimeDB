use std::path::PathBuf;

use anyhow::Context;
use clap::{ArgMatches, CommandFactory, FromArgMatches};
use spacetimedb::host::extract_schema;
use spacetimedb::messages::control_db;
use spacetimedb_lib::{sats, RawModuleDef};

/// Extracts the module schema from a local module file.
/// WARNING: This command is UNSTABLE and subject to breaking changes.
#[derive(clap::Parser)]
#[command(name = "extract-schema")]
struct Args {
    /// The module file
    module: PathBuf,

    /// The type of module
    #[arg(long)]
    host_type: Option<HostType>,
}

#[derive(clap::ValueEnum, Copy, Clone)]
enum HostType {
    Wasm,
    Js,
}

impl From<HostType> for control_db::HostType {
    fn from(x: HostType) -> Self {
        match x {
            HostType::Wasm => control_db::HostType::Wasm,
            HostType::Js => control_db::HostType::Js,
        }
    }
}

impl HostType {
    fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "wasm" => Some(Self::Wasm),
            "js" => Some(Self::Js),
            _ => None,
        }
    }
}

pub fn cli() -> clap::Command {
    Args::command()
}

pub async fn exec(args: &ArgMatches) -> anyhow::Result<()> {
    let args = Args::from_arg_matches(args)?;

    let host_type = match args.host_type {
        Some(x) => x,
        None => args
            .module
            .extension()
            .and_then(|x| x.to_str())
            .and_then(HostType::from_extension)
            .context("--host-type not provided but cannot deduce from file extension")?,
    };

    let program_bytes = tokio::fs::read(&args.module)
        .await
        .with_context(|| format!("could not read module file {}", args.module.display()))?;

    let module_def = extract_schema(program_bytes.into(), host_type.into()).await?;

    let raw_def = RawModuleDef::V9(module_def.into());

    serde_json::to_writer(std::io::stdout().lock(), &sats::serde::SerdeWrapper(raw_def))?;

    Ok(())
}
