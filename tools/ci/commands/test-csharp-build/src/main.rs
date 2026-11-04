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

    let version = prepare_packages(work).context("Failed to prepare runtime packages")?;

    for template in ["basic-cs", "chat-console-cs"] {
        check_template(work, &cli, template, &version)
            .with_context(|| format!("Build checks failed for {template}"))?;
    }
    check_test_references(work, &version).context("Test-project reference checks failed")?;
    check_net8(work, &cli).context(".NET 8 WASM build check failed")?;
    Ok(())
}

fn prepare_packages(work: &Path) -> Result<String> {
    let packages = work.join("packages");
    let version = if let Some(feed) = std::env::var_os("CSHARP_PACKAGE_FEED").filter(|feed| !feed.is_empty()) {
        let version =
            std::env::var("CSHARP_PACKAGE_VERSION").context("Set CSHARP_PACKAGE_VERSION with CSHARP_PACKAGE_FEED")?;
        ensure!(!version.is_empty(), "CSHARP_PACKAGE_VERSION must not be empty");
        copy_packages(Path::new(&feed), &packages)?;
        version
    } else {
        let version = format!(
            "0.0.0-buildtest.{}.{}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
            std::process::id()
        );
        pack_runtime(work, &version)?;
        version
    };
    write_nuget_config(work)?;
    Ok(version)
}

fn copy_packages(feed: &Path, packages: &Path) -> Result<()> {
    fs::create_dir_all(packages)?;
    let mut copied = 0;
    for entry in fs::read_dir(feed).with_context(|| format!("Failed to read package feed {}", feed.display()))? {
        let entry = entry?;
        if entry.file_type()?.is_file() && entry.path().extension().is_some_and(|extension| extension == "nupkg") {
            fs::copy(entry.path(), packages.join(entry.file_name()))?;
            copied += 1;
        }
    }
    ensure!(copied > 0, "No NuGet packages found in {}", feed.display());
    Ok(())
}

fn run(work: &Path, command: duct::Expression) -> Result<()> {
    command
        .env("NUGET_PACKAGES", work.join("nuget-packages"))
        .env("NUGET_HTTP_CACHE_PATH", work.join("http-cache"))
        .run()?;
    Ok(())
}

fn pack_runtime(work: &Path, version: &str) -> Result<()> {
    let packages = work.join("packages");
    for project in ["BSATN.Runtime", "Runtime"] {
        run(
            work,
            cmd!(
                "dotnet",
                "pack",
                format!("crates/bindings-csharp/{project}"),
                "-c",
                "Release",
                format!("-p:Version={version}"),
                "-o",
                &packages
            ),
        )?;
    }
    Ok(())
}

fn write_nuget_config(work: &Path) -> Result<()> {
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
    run(
        work,
        cmd!(
            "dotnet",
            "add",
            &project,
            "package",
            "SpacetimeDB.Runtime",
            "--version",
            version,
            "--no-restore"
        ),
    )?;
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

    check_managed(work, &module, &consumer).context("Managed build before WASM failed")?;
    run(work, cmd!(cli, "build", "--module-path", &module)).context("WASM build failed")?;
    let assets: serde_json::Value = serde_json::from_reader(File::open(module.join("obj/project.assets.json"))?)?;
    ensure!(
        assets["targets"]["net10.0/wasi-wasm"][format!("SpacetimeDB.Runtime/{version}")]["runtime"]
            .get("runtimes/wasi-wasm/lib/net10.0/SpacetimeDB.Runtime.dll")
            .is_some(),
        "WASM publish did not select the host-only Runtime"
    );
    check_no_testing_package(&assets)?;
    ensure!(
        !has_native_testing_asset(&assets),
        "Native testing assets leaked into WASM restore assets"
    );
    check_no_testing_outputs(&module.join("bin"))?;
    check_managed(work, &module, &consumer).context("Managed build after WASM failed")?;
    Ok(())
}

fn check_managed(work: &Path, module: &Path, consumer: &Path) -> Result<()> {
    // A legacy/global AOT flag must not turn a .NET 10 managed build into WASM.
    run(
        work,
        cmd!(
            "dotnet",
            "run",
            "--project",
            consumer.join("Consumer.csproj"),
            "-c",
            "Release"
        )
        .env("SPACETIMEDB_WASM_AOT", "1"),
    )?;
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
    check_no_testing_package(&assets)?;
    Ok(())
}

fn check_no_testing_package(assets: &serde_json::Value) -> Result<()> {
    let libraries = assets["libraries"]
        .as_object()
        .context("Missing libraries in project.assets.json")?;
    ensure!(
        !libraries.keys().any(|name| name.starts_with("SpacetimeDB.Testing/")),
        "Module restored SpacetimeDB.Testing"
    );
    Ok(())
}

fn has_native_testing_asset(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(fields) => fields
            .iter()
            .any(|(name, value)| name.contains("spacetimedb_test_datastore_ffi") || has_native_testing_asset(value)),
        serde_json::Value::Array(values) => values.iter().any(has_native_testing_asset),
        _ => false,
    }
}

fn check_no_testing_outputs(directory: &Path) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            check_no_testing_outputs(&entry.path())?;
        } else {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            ensure!(
                !name.contains("spacetimedb.testing") && !name.contains("spacetimedb_test_datastore_ffi"),
                "Testing asset leaked into module build output: {}",
                entry.path().display()
            );
        }
    }
    Ok(())
}

fn check_test_references(work: &Path, version: &str) -> Result<()> {
    println!("Checking test-project references to a module project and package");
    let module = work.join("chat-console-cs/module/StdbModule.csproj");
    let tests = work.join("chat-console-cs/tests");
    let project = tests.join("Module.Tests.csproj");
    fs::create_dir_all(&tests)?;
    fs::write(
        &project,
        r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net10.0</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <IsTestProject>true</IsTestProject>
    <IsPackable>false</IsPackable>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="Microsoft.NET.Test.Sdk" Version="17.6.0" />
    <PackageReference Include="xunit" Version="2.9.0" />
    <PackageReference Include="xunit.runner.visualstudio" Version="2.8.1" PrivateAssets="all" />
    <ProjectReference Include="../module/StdbModule.csproj" />
  </ItemGroup>
</Project>
"#,
    )?;
    fs::write(
        tests.join("ModuleReferenceTests.cs"),
        r#"using System.Reflection;
using SpacetimeDB;
using Xunit;

public class ModuleReferenceTests
{
    [Fact]
    public void ReferencesModuleWithoutBecomingAModule()
    {
        var testAssembly = typeof(ModuleReferenceTests).Assembly;
        Assert.Null(testAssembly.GetCustomAttribute<ModuleDescriptorAttribute>());
        Assert.Null(testAssembly.GetType("ModuleRegistration"));
    }

    // Compile-time checks only; accessing live data still requires the testing backend.
    public static ulong Count(ReducerContext ctx) => ctx.Db.Message.Count;
    public static ulong Count(ViewContext ctx) => ctx.Db.Message.Count;
    public static IQuery<global::Module.Message> Query(ViewContext ctx) => ctx.From.Message();
}
"#,
    )?;
    run(work, cmd!("dotnet", "test", &project, "-c", "Release")).context("Module project reference failed")?;
    run(
        work,
        cmd!(
            "dotnet",
            "pack",
            &module,
            "-c",
            "Release",
            "-p:PackageId=BuildTest.Module",
            format!("-p:Version={version}"),
            "-o",
            work.join("packages")
        ),
    )?;
    run(work, cmd!("dotnet", "remove", &project, "reference", &module))?;
    run(
        work,
        cmd!(
            "dotnet",
            "add",
            &project,
            "package",
            "BuildTest.Module",
            "--version",
            version
        ),
    )?;
    run(work, cmd!("dotnet", "test", &project, "-c", "Release")).context("Module package reference failed")?;
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
    run(
        work,
        cmd!(cli, "build", "--module-path", &module, "--dotnet-version", "8"),
    )?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn copies_only_packages_and_rejects_empty_feeds() -> Result<()> {
        let feed = tempfile::tempdir()?;
        let work = tempfile::tempdir()?;
        let packages = work.path().join("packages");
        assert!(copy_packages(feed.path(), &packages).is_err());
        fs::write(feed.path().join("Runtime.nupkg"), b"package")?;
        fs::write(feed.path().join("README.txt"), b"not a package")?;
        copy_packages(feed.path(), &packages)?;
        assert_eq!(fs::read(packages.join("Runtime.nupkg"))?, b"package");
        assert!(!packages.join("README.txt").exists());
        Ok(())
    }

    #[test]
    fn rejects_testing_dependencies_and_nested_native_assets() {
        assert!(check_no_testing_package(&json!({"libraries": {"SpacetimeDB.Runtime/1.0": {}}})).is_ok());
        assert!(check_no_testing_package(&json!({"libraries": {"SpacetimeDB.Testing/1.0": {}}})).is_err());
        assert!(check_no_testing_package(&json!({})).is_err());
        assert!(has_native_testing_asset(&json!({"targets": [{"native": {
            "runtimes/linux-x64/native/libspacetimedb_test_datastore_ffi.so": {}
        }}]})));
        assert!(!has_native_testing_asset(
            &json!({"runtime": {"SpacetimeDB.Runtime.dll": {}}})
        ));
    }

    #[test]
    fn rejects_testing_outputs_recursively_and_case_insensitively() -> Result<()> {
        for name in ["SpacetimeDB.Testing.dll", "LIBSPACETIMEDB_TEST_DATASTORE_FFI.SO"] {
            let output = tempfile::tempdir()?;
            let nested = output.path().join("Release/net10.0");
            fs::create_dir_all(&nested)?;
            fs::write(nested.join("SpacetimeDB.Runtime.dll"), b"runtime")?;
            check_no_testing_outputs(output.path())?;
            fs::write(nested.join(name), b"testing")?;
            assert!(check_no_testing_outputs(output.path()).is_err());
        }
        Ok(())
    }
}
