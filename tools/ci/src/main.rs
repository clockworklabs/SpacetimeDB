use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use duct::cmd;
use std::collections::HashMap;
use std::env;

#[derive(Parser)]
#[command(
    name = "spacetimedb-ci",
    about = "SpacetimeDB CI tasks",
    subcommand_required = false,
    arg_required_else_help = false
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<CiCmd>,

    #[arg(long)]
    skip: Vec<String>,
}

#[derive(Subcommand)]
enum CiCmd {
    Test,
    Lints,
    WasmBindings,
    Smoketests {
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    UpdateFlow {
        #[arg(long)]
        target: Option<String>,
        #[arg(long, default_value = "true")]
        github_token_auth: bool,
    },
    UnrealTests,
    CliDocs,
}

macro_rules! run {
    ($cmdline:expr) => {
        run_command($cmdline, &Vec::new())
    };
    ($cmdline:expr, $envs:expr) => {
        run_command($cmdline, $envs)
    };
}

fn run_all_clap_subcommands(skips: &[String]) -> Result<()> {
    let subcmds = Cli::command()
        .get_subcommands()
        .map(|sc| sc.get_name().to_string())
        .collect::<Vec<_>>();

    for subcmd in subcmds {
        if skips.contains(&subcmd) {
            log::info!("skipping {subcmd} as requested");
            continue;
        }
        log::info!("executing cargo ci {subcmd}");
        run!(&format!("cargo ci {subcmd}"))?;
    }

    Ok(())
}

fn run_command(cmdline: &str, additional_env: &[(&str, &str)]) -> Result<()> {
    let mut env = env::vars().collect::<HashMap<_, _>>();
    env.extend(additional_env.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    log::debug!("$ {cmdline}");
    let status = cmd!("bash", "-lc", cmdline).full_env(env).run()?;
    if !status.status.success() {
        let e = anyhow::anyhow!("command failed: {cmdline}");
        log::error!("{e}");
        return Err(e);
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Some(CiCmd::Test) => {
            run!("sudo mkdir -p /stdb && sudo chmod 777 /stdb")?;
            run!("cargo test --all -- --skip unreal")?;
            run!("bash tools/check-diff.sh")?;
            run!("cargo run -p spacetimedb-codegen --example regen-csharp-moduledef && bash tools/check-diff.sh crates/bindings-csharp")?;
            run!("(cd crates/bindings-csharp && dotnet test -warnaserror)")?;
        }

        Some(CiCmd::Lints) => {
            run!("cargo fmt --all -- --check")?;
            run!("cargo clippy --all --tests --benches -- -D warnings")?;
            run!("(cd crates/bindings-csharp && dotnet tool restore && dotnet csharpier --check .)")?;
            run!("cd crates/bindings && cargo doc", &[("RUSTDOCFLAGS", "hey")])?;
        }

        Some(CiCmd::WasmBindings) => {
            run!("cargo test -p spacetimedb-codegen")?;
            run!("cargo update")?;
            run!("cargo run -p spacetimedb-cli -- build --project-path modules/module-test")?;
        }

        Some(CiCmd::Smoketests { args }) => {
            // Note: clear_database and replication only work in private
            run!(&format!("python -m smoketests {} -x clear_database replication", args.join(" ")))?;
        }

        Some(CiCmd::UpdateFlow {
            target,
            github_token_auth,
        }) => {
            let target = target.unwrap_or_else(|| env!("TARGET").to_string());
            let github_token_auth_flag = if github_token_auth {
                "--features github-token-auth "
            } else {
                ""
            };

            run!(&format!("echo 'checking update flow for target: {target}'"))?;
            run!(&format!(
                "cargo build {github_token_auth_flag}--target {target} -p spacetimedb-update"
            ))?;
            run!(&format!(
                r#"
ROOT_DIR="$(mktemp -d)"
# NOTE(bfops): We need the `github-token-auth` feature because we otherwise tend to get ratelimited when we try to fetch `/releases/latest`.
# My best guess is that, on the GitHub runners, the "anonymous" ratelimit is shared by *all* users of that runner (I think this because it
# happens very frequently on the `macos-runner`, but we haven't seen it on any others).
cargo run {github_token_auth_flag}--target {target} -p spacetimedb-update -- self-install --root-dir="${{ROOT_DIR}}" --yes
"${{ROOT_DIR}}"/spacetime --root-dir="${{ROOT_DIR}}" help
        "#
            ))?;
        }

        Some(CiCmd::UnrealTests) => {
            run!("for p in \"$GITHUB_WORKSPACE\" \"${RUNNER_TEMP:-/__t}\" \"${RUNNER_TOOL_CACHE:-/__t}\"; do [ -d \"$p\" ] && setfacl -R -m u:ue4:rwX -m d:u:ue4:rwX \"$p\" || true; done")?;

            run!("export CARGO_HOME=\"${RUNNER_TOOL_CACHE:-/__t}/cargo\"")?;
            run!("export RUSTUP_HOME=\"${RUNNER_TOOL_CACHE:-/__t}/rustup\"")?;
            run!("mkdir -p \"$CARGO_HOME\" \"$RUSTUP_HOME\"")?;

            run!("chmod a+rx \"$UE_ROOT_PATH\" \"$UE_ROOT_PATH/Engine\" \"$UE_ROOT_PATH/Engine/Build\" \"$UE_ROOT_PATH/Engine/Build/BatchFiles/Linux\" || true")?;
            run!("chmod a+rx \"$UE_ROOT_PATH/Engine/Build/BatchFiles/Linux/Build.sh\" || true")?;

            run!("sudo -E -H -u ue4 env HOME=/home/ue4 CARGO_HOME=\"$CARGO_HOME\" RUSTUP_HOME=\"$RUSTUP_HOME\" PATH=\"$CARGO_HOME/bin:$PATH\" bash -lc 'set -euxo pipefail; if ! command -v cargo >/dev/null 2>&1; then curl -sSf https://sh.rustup.rs | sh -s -- -y; fi; rustup show >/dev/null; git config --global --add safe.directory \"$GITHUB_WORKSPACE\" || true; cd \"$GITHUB_WORKSPACE/sdks/unreal\"; cargo --version; cargo test'")?;
        }

        Some(CiCmd::CliDocs) => {
            run!("pnpm install --recursive")?;
            run!("cargo run --features markdown-docs -p spacetimedb-cli > docs/docs/cli-reference.md")?;
            run!("pnpm format")?;
            run!("git status")?;
            run!(
                r#"
if git diff --exit-code HEAD; then
  echo "No docs changes detected"
else
  echo "It looks like the CLI docs have changed:"
  exit 1
fi
                "#
            )?;
        }

        None => run_all_clap_subcommands(&cli.skip)?,
    }

    Ok(())
}
