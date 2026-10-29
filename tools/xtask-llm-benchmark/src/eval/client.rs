//! Actual SDK client checks. The fixed harness cuts a real connection after a committed call.
use super::{derive_cat_task_from_file, Lang, ScoreDetails, Scorer};
use crate::bench::{runner::with_build_cleanup, templates::materialize_client, utils::sanitize_db_name};
use anyhow::{ensure, Result};
use serde_json::json;
use std::{path::Path, process::Command, time::Duration};

pub fn reconnect_client(source: &str, route: &str, server: &str, lang: Lang) -> Box<dyn Scorer> {
    let (category, task) = derive_cat_task_from_file(source);
    Box::new(ClientCheck {
        category,
        task,
        route: route.into(),
        server: server.into(),
        lang,
    })
}

struct ClientCheck {
    category: String,
    task: String,
    route: String,
    server: String,
    lang: Lang,
}

fn run(cmd: Command, cwd: &Path) -> Result<String> {
    let (code, stdout, stderr) = super::utils::run_with_timeout(cmd, cwd, Duration::from_secs(180))?;
    let output = format!(
        "{}\n{}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr)
    );
    ensure!(code == 0, "client check exited {code}: {output}");
    Ok(output)
}

/// Fail setup before any model request if the client sandbox is unavailable.
pub(crate) fn check_runtime(lang: Lang) -> Result<()> {
    // C# also uses the fixed Node relay, with no network interface or host listener.
    if lang == Lang::CSharp {
        check_runtime(Lang::TypeScript)?;
    }
    let (image, executable, argument) = match lang {
        Lang::TypeScript => ("node:22-bookworm-slim", "node", "--version"),
        Lang::CSharp => ("mcr.microsoft.com/dotnet/runtime:10.0", "dotnet", "--list-runtimes"),
        Lang::Rust => anyhow::bail!("Rust client recovery is not supported"),
    };
    let mut command = Command::new("docker");
    command.args([
        "run",
        "--rm",
        "--pull=never",
        "--network=none",
        "--read-only",
        "--cap-drop=ALL",
        "--security-opt=no-new-privileges",
        "--user=65534:65534",
        image,
        "timeout",
        "10",
        executable,
        argument,
    ]);
    run(command, Path::new(env!("CARGO_MANIFEST_DIR")))?;
    Ok(())
}

impl ClientCheck {
    fn check(&self, source: &str) -> Result<String> {
        let project = std::path::absolute(materialize_client(
            self.lang.as_str(),
            &self.category,
            &self.task,
            &self.route,
            source,
        )?)?;
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap();
        let database = sanitize_db_name(&format!("{}-{}-{}-llm", self.category, self.task, self.route));
        with_build_cleanup(&project, self.lang, || {
            let mut command = Command::new("node");
            command
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/eval/client_harness.mjs"))
                .args([&self.server, &database]);
            match self.lang {
                Lang::TypeScript => {
                    let mut build = Command::new("node");
                    build
                        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/eval/build_client.mjs"))
                        .arg(workspace.join("crates/bindings-typescript"))
                        .arg(&project);
                    run(build, &project)?;
                    command.arg("typescript").arg(project.join("dist"));
                }
                Lang::CSharp => {
                    let mut build = Command::new("dotnet");
                    build.args(["build", "client.csproj", "--configuration", "Release"]);
                    build.arg(format!(
                        "-p:RestoreConfigFile={}",
                        project.join("nuget.config").display()
                    ));
                    build.arg(format!(
                        "-p:RestorePackagesPath={}",
                        project.join(".nuget/packages").display()
                    ));
                    run(build, &project)?;
                    command.arg("csharp").arg(project.join("bin/Release/net10.0"));
                }
                Lang::Rust => anyhow::bail!("Rust client recovery is not supported by this eval"),
            }
            let output = run(command, &project)?;
            ensure!(
                output.lines().any(|line| line == "CLIENT_RECOVERY_OK"),
                "client assertions did not finish: {output}"
            );
            Ok(output)
        })
    }
}

impl Scorer for ClientCheck {
    fn id(&self) -> &'static str {
        "sdk_reconnect_unknown_result"
    }
    fn score(&self, output: &str) -> ScoreDetails {
        let result = self.check(output);
        ScoreDetails {
            pass: result.is_ok(),
            partial: if result.is_ok() { 1.0 } else { 0.0 },
            notes: match result {
                Ok(output) => json!({"checks": output}),
                Err(error) => json!({"error": format!("{error:#}")}),
            },
        }
    }
}
