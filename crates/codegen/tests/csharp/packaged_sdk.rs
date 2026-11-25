use super::{compiled_typescript_module, module_artifact};
use spacetimedb_codegen::{generate, CodegenOptions, Csharp};
use spacetimedb_paths::{RootDir, SpacetimePaths};
use spacetimedb_schema::def::ModuleDef;
use std::{path::Path, process::Command};

/// A package consumer gets the analyzer only through the SDK's transitive NuGet dependency.
#[test]
fn packaged_sdk_builds_the_same_client_after_regeneration() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let feed = root.join("feed");
    let client = root.join("client");
    let bindings = client.join("module_bindings");
    fs_err::create_dir_all(&feed).unwrap();
    fs_err::create_dir_all(&bindings).unwrap();

    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = root.join("NuGet.Config");
    fs_err::write(
        &config,
        r#"<configuration>
  <packageSources>
    <clear />
    <add key="local" value="feed" />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
  </packageSources>
  <packageSourceMapping>
    <packageSource key="local"><package pattern="SpacetimeDB.*" /></packageSource>
    <packageSource key="nuget.org"><package pattern="*" /></packageSource>
  </packageSourceMapping>
</configuration>"#,
    )
    .unwrap();
    let config_arg = format!("-p:RestoreConfigFile={}", config.display());
    let cache_arg = format!("-p:RestorePackagesPath={}", root.join("packages").display());
    for project in [
        "crates/bindings-csharp/BSATN.Runtime/BSATN.Runtime.csproj",
        "sdks/csharp/SpacetimeDB.ClientSDK.csproj",
    ] {
        let mut pack = Command::new("dotnet");
        pack.arg("pack")
            .arg(repo.join(project))
            .args(["-c", "Release", "-o"])
            .arg(&feed)
            .arg(&config_arg);
        if project.starts_with("sdks/") {
            pack.arg(&cache_arg);
        }
        run(&mut pack);
    }

    fs_err::write(
        client.join("client.csproj"),
        r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <LangVersion>9</LangVersion>
    <Nullable>enable</Nullable>
    <OutputType>Exe</OutputType>
  </PropertyGroup>
  <ItemGroup><PackageReference Include="SpacetimeDB.ClientSDK" Version="*" /></ItemGroup>
</Project>"#,
    )
    .unwrap();
    let program = client.join("Program.cs");
    fs_err::write(&program, include_str!("Program.cs")).unwrap();
    let mut run_client = Command::new("dotnet");
    run_client
        .current_dir(&client)
        .arg("run")
        .arg(&cache_arg)
        .arg(&config_arg)
        .arg("-p:TreatWarningsAsErrors=true");

    let old_bindings = bindings.join("OldBindings.cs");
    fs_err::write(&old_bindings, include_str!("OldBindings.cs")).unwrap();
    run(&mut run_client);
    let cli = SpacetimePaths::from_root_dir(&RootDir(root.join("spacetime")));
    let mut generate = vec![
        "generate",
        "--lang",
        "csharp",
        "--namespace",
        "SpacetimeDB",
        "--bin-path",
        module_artifact().path().to_str().unwrap(),
        "--out-dir",
        bindings.to_str().unwrap(),
        "--no-config",
        "-y",
    ];
    spacetimedb_testing::invoke_cli(&cli, &generate);
    assert!(
        !old_bindings.exists(),
        "regeneration left obsolete bindings in the project"
    );
    run(&mut run_client);

    check_invalid_arguments(&client, &cache_arg);

    generate.push("--include-private");
    spacetimedb_testing::invoke_cli(&cli, &generate);
    let private_client = client.join("PrivateTableClient.cs");
    fs_err::write(&private_client, "using SpacetimeDB; class PrivateTableClient { RemoteTables.PrivateTableHandle Table(DbConnection db) => db.Db.PrivateTable; }").unwrap();
    run(&mut run_client);
    fs_err::remove_file(private_client).unwrap();

    fs_err::remove_dir_all(&bindings).unwrap();
    write_bindings(&bindings, compiled_typescript_module());
    fs_err::write(program, include_str!("Submodule.cs")).unwrap();
    run(&mut run_client);
}

fn check_invalid_arguments(client: &Path, cache_arg: &str) {
    let calls = [
        ("BadReducer", r#"db.Reducers.Add("Alice", "42")"#),
        (
            "BadProcedure",
            r#"db.Procedures.ReturnValue("42", (ctx, result) => {})"#,
        ),
    ];
    for (name, call) in calls {
        fs_err::write(
            client.join(format!("{name}.cs")),
            format!("using SpacetimeDB; class {name} {{ void Call(DbConnection db) => {call}; }}"),
        )
        .unwrap();
    }
    let result = Command::new("dotnet")
        .current_dir(client)
        .args(["build", "--no-restore", cache_arg])
        .output()
        .unwrap();
    let diagnostics = String::from_utf8_lossy(&result.stdout);
    assert!(!result.status.success(), "invalid reducer and procedure calls compiled");
    for (name, _) in calls {
        let file = format!("{name}.cs");
        assert!(
            diagnostics
                .lines()
                .any(|line| line.contains(&file) && line.contains("error CS1503")),
            "{diagnostics}"
        );
        fs_err::remove_file(client.join(file)).unwrap();
    }
}

fn write_bindings(dir: &Path, module: &ModuleDef) {
    for file in generate(
        module,
        &Csharp {
            namespace: "SpacetimeDB",
        },
        &CodegenOptions::default(),
    ) {
        let path = dir.join(file.filename);
        fs_err::create_dir_all(path.parent().unwrap()).unwrap();
        fs_err::write(path, file.code).unwrap();
    }
}

fn run(command: &mut Command) {
    let output = command.output().expect("dotnet SDK is required");
    assert!(
        output.status.success(),
        "{command:?} failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
