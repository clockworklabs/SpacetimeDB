#![allow(clippy::disallowed_macros)]

use anyhow::{ensure, Context, Result};
use duct::cmd;
use std::fs::{self, File};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use xmltree::{Element, XMLNode};

// Requires .NET 10, .NET 8 with wasi-experimental, and a CLI built from this checkout.
fn main() -> Result<()> {
    ci_common::ensure_repo_root()?;
    let cli = ci_common::require_spacetime_bin()?;
    let temp_dir = tempfile::tempdir()?;
    let work = temp_dir.path();

    // A unique version prevents NuGet's cache from hiding changes to the runtime.
    let version = format!(
        "0.0.0-buildtest.{}.{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
        std::process::id()
    );
    pack_runtime(work, &version).context("Failed to prepare local runtime packages")?;

    for template in ["basic-cs", "chat-console-cs"] {
        check_template(work, &cli, template, &version)
            .with_context(|| format!("Build checks failed for {template}"))?;
    }
    check_net8(work, &cli).context(".NET 8 WASM build check failed")?;
    Ok(())
}

fn pack_runtime(work: &Path, version: &str) -> Result<()> {
    let packages = work.join("packages");
    for project in ["BSATN.Runtime", "Runtime"] {
        cmd!(
            "dotnet",
            "pack",
            format!("crates/bindings-csharp/{project}"),
            "-c",
            "Release",
            format!("-p:Version={version}"),
            "-o",
            &packages
        )
        .run()?;
    }
    fs::write(
        work.join("NuGet.Config"),
        r#"<configuration>
  <packageSources>
    <clear />
    <add key="local" value="packages" />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
    <add key="dotnet-experimental" value="https://pkgs.dev.azure.com/dnceng/public/_packaging/dotnet-experimental/nuget/v3/index.json" />
  </packageSources>
</configuration>
"#,
    )?;
    Ok(())
}

fn check_template(work: &Path, cli: &Path, template: &str, version: &str) -> Result<()> {
    println!("Checking {template} managed/WASM builds");
    let source = Path::new("templates").join(template).join("spacetimedb");
    let module = work.join(template).join("module");
    let consumer = work.join(template).join("consumer");
    fs::create_dir_all(&module)?;
    fs::create_dir_all(&consumer)?;
    fs::copy(source.join("Lib.cs"), module.join("Lib.cs"))?;
    fs::copy(source.join("global.json"), work.join(template).join("global.json"))?;
    let project = module.join("StdbModule.csproj");
    // Match the framework selection performed by spacetime init.
    set_framework(
        &source.join("StdbModule.csproj"),
        &project,
        "TargetFrameworks",
        "TargetFramework",
        "net10.0",
    )?;
    cmd!(
        "dotnet",
        "add",
        &project,
        "package",
        "SpacetimeDB.Runtime",
        "--version",
        version,
        "--no-restore"
    )
    .run()?;
    fs::write(
        consumer.join("Consumer.csproj"),
        r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net10.0</TargetFramework>
    <OutputType>Exe</OutputType>
    <ImplicitUsings>enable</ImplicitUsings>
  </PropertyGroup>
  <ItemGroup>
    <ProjectReference Include="../module/StdbModule.csproj" />
  </ItemGroup>
</Project>
"#,
    )?;
    fs::write(
        consumer.join("Program.cs"),
        r#"using System.Reflection;
using System.Runtime.InteropServices;

var registration = typeof(Module).Assembly.GetType("ModuleRegistration", throwOnError: true)!;
if (registration.GetMethods().Any(method => method.IsDefined(typeof(UnmanagedCallersOnlyAttribute))))
    throw new Exception("Managed module contains WASM-only entry points");
Console.WriteLine("Managed module reference works");
"#,
    )?;

    check_managed(&module, &consumer).context("Managed build before WASM failed")?;
    cmd!(cli, "build", "--module-path", &module)
        .run()
        .context("WASM build failed")?;
    check_managed(&module, &consumer).context("Managed build after WASM failed")?;
    Ok(())
}

fn check_managed(module: &Path, consumer: &Path) -> Result<()> {
    // A legacy/global AOT flag must not turn a .NET 10 managed build into WASM.
    cmd!(
        "dotnet",
        "run",
        "--project",
        consumer.join("Consumer.csproj"),
        "-c",
        "Release"
    )
    .env("SPACETIMEDB_WASM_AOT", "1")
    .run()?;
    let assets_path = module.join("obj/project.assets.json");
    let assets: serde_json::Value = serde_json::from_reader(
        File::open(&assets_path).with_context(|| format!("Failed to open {}", assets_path.display()))?,
    )?;
    let libraries = assets["libraries"]
        .as_object()
        .context("Missing libraries in project.assets.json")?;
    ensure!(
        !libraries.keys().any(|name| name.contains("ILCompiler.LLVM")),
        "Managed module restored WASM-only ILCompiler.LLVM packages"
    );
    Ok(())
}

fn check_net8(work: &Path, cli: &Path) -> Result<()> {
    // Exercise the .NET 8 SDK's WASI import order, not just our final property values.
    let module = work.join("net8/module");
    fs::create_dir_all(&module)?;
    fs::copy("templates/chat-console-cs/spacetimedb/Lib.cs", module.join("Lib.cs"))?;
    set_framework(
        &work.join("chat-console-cs/module/StdbModule.csproj"),
        &module.join("StdbModule.csproj"),
        "TargetFramework",
        "TargetFrameworks",
        "net8.0;net10.0",
    )?;
    fs::write(
        work.join("net8/global.json"),
        r#"{"sdk":{"version":"8.0.100","rollForward":"latestFeature"}}"#,
    )?;
    cmd!(cli, "build", "--module-path", &module, "--dotnet-version", "8").run()?;
    Ok(())
}

fn set_framework(source: &Path, destination: &Path, old_name: &str, new_name: &str, value: &str) -> Result<()> {
    let mut project = Element::parse(File::open(source)?)?;
    let framework = project
        .children
        .iter_mut()
        .filter_map(XMLNode::as_mut_element)
        .filter(|element| element.name == "PropertyGroup")
        .find_map(|group| group.get_mut_child(old_name))
        .with_context(|| format!("Missing {old_name} in {}", source.display()))?;
    framework.name = new_name.to_owned();
    framework.children = vec![XMLNode::Text(value.to_owned())];
    project.write(File::create(destination)?)?;
    Ok(())
}
