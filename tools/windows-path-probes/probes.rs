//! Diagnostic experiment, not intended to merge. No repository crates are built.
//! Each test first verifies an ordinary path, then exercises the same tool with
//! a canonicalized path, as used by the call sites referenced in README.md.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "spacetime path probes {} {} {}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        // Keep SDK selection consistent with the repository, even outside it.
        fs::write(
            path.join("global.json"),
            r#"{"sdk":{"version":"10.0.100","rollForward":"latestMinor"}}"#,
        )
        .unwrap();
        Self(path)
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("Failed fixture retained at {}", self.0.display());
        } else {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn canonical(path: &Path) -> PathBuf {
    let path = path.canonicalize().unwrap();
    #[cfg(windows)]
    assert!(
        path.to_str().unwrap().starts_with(r"\\?\"),
        "The Windows probe must actually exercise an extended path: {path:?}"
    );
    eprintln!("Canonical input: {}", path.display());
    path
}

fn run(command: &mut Command) -> Output {
    eprintln!("Running {command:?}");
    let output = command.output().expect("required test tool must be installed");
    eprintln!(
        "status={}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn succeeds(command: &mut Command) {
    assert!(run(command).status.success(), "external tool failed");
}

fn dotnet(fixture: &Fixture) -> Command {
    let mut command = Command::new("dotnet");
    command.current_dir(&fixture.0);
    command
}

// JSON string literals also encode the ordinary paths used in Cargo's TOML.
fn quoted(path: &Path) -> String {
    format!(
        "\"{}\"",
        path.to_str().unwrap().replace('\\', "\\\\").replace('"', "\\\"")
    )
}

fn csproj(fixture: &Fixture, framework: &str) -> PathBuf {
    fixture.write(
        "library/Library.csproj",
        &format!(
            r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup><TargetFramework>{framework}</TargetFramework></PropertyGroup>
</Project>"#
        ),
    )
}

#[test]
fn msbuild_wildcard_import_from_canonical_project() {
    let fixture = Fixture::new();
    fixture.write(
        "project/found.props",
        "<Project><PropertyGroup><Found>true</Found></PropertyGroup></Project>",
    );
    let project = fixture.write(
        "project/Test.proj",
        r#"<Project>
  <Import Project="$(MSBuildProjectDirectory)/*.props" />
  <Target Name="Probe">
    <Error Condition="'$(Found)' != 'true'" Text="The props file was not imported" />
  </Target>
</Project>"#,
    );
    for project in [project.clone(), canonical(&project)] {
        succeeds(dotnet(&fixture).arg("msbuild").arg(project).arg("-t:Probe"));
    }
}

#[test]
fn dotnet_pack_canonical_project_directory() {
    let fixture = Fixture::new();
    let project = csproj(&fixture, "net8.0");
    fixture.write("library/Class.cs", "public class Example {}\n");
    // A real restore/pack control also creates the generated NuGet props imports.
    succeeds(
        dotnet(&fixture)
            .arg("pack")
            .arg(project.parent().unwrap())
            .args(["-c", "Release"]),
    );
    succeeds(
        dotnet(&fixture)
            .arg("pack")
            .arg(canonical(project.parent().unwrap()))
            .args(["-c", "Release"]),
    );
}

#[test]
fn dotnet_restore_canonical_netstandard_reference() {
    let fixture = Fixture::new();
    let project = csproj(&fixture, "netstandard2.0");
    // Matches the CLI's extra restore of canonicalized netstandard2.0 references
    // in its .NET 8 build path, without requiring a WASI workload or publication.
    succeeds(dotnet(&fixture).arg("restore").arg(&project).arg("--force"));
    succeeds(dotnet(&fixture).arg("restore").arg(canonical(&project)).arg("--force"));
}

#[test]
fn dotnet_build_canonical_project_reference() {
    let fixture = Fixture::new();
    let reference = csproj(&fixture, "net8.0");
    fixture.write("library/Class.cs", "public class Example {}\n");
    fixture.write("client/Class.cs", "public class Client : Example {}\n");
    for reference in [reference.clone(), canonical(&reference)] {
        let project = fixture.write(
            "client/Client.csproj",
            &format!(
                r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup>
  <ItemGroup><ProjectReference Include="{}" /></ItemGroup>
</Project>"#,
                reference.display()
            ),
        );
        succeeds(dotnet(&fixture).arg("build").arg(project));
    }
}

#[test]
fn dotnet_format_canonical_included_file() {
    let fixture = Fixture::new();
    let source = "class Example{void Method(){int value=1;}}\n";
    let file = fixture.write("format/Example.cs", source);
    let folder = file.parent().unwrap();
    let format = |included: &Path| {
        succeeds(dotnet(&fixture).args([
            "format",
            "whitespace",
            "--folder",
            folder.to_str().unwrap(),
            "--include-generated",
            "--include",
            included.to_str().unwrap(),
        ]));
        fs::read_to_string(&file).unwrap()
    };
    let expected = format(&file);
    assert_ne!(expected, source, "control must actually format the file");
    fs::write(&file, source).unwrap();
    let actual = format(&canonical(&file));
    assert_eq!(actual, expected, "a successful exit must not hide a skipped file");
}

fn typescript_probe(canonical_extends: bool, canonical_paths: bool, canonical_cwd: bool) {
    let fixture = Fixture::new();
    fixture.write("package.json", r#"{"private":true}"#);
    let sdk = fixture.write(
        "sdk/tsconfig.json",
        r#"{"compilerOptions":{"strict":true,"noEmit":true}}"#,
    );
    let dependency = fixture.write("sdk/index.ts", "export const value: number = 42;\n");
    fixture.write(
        "client/main.ts",
        "import { value } from 'probe-sdk'; const result: number = value;\n",
    );
    let config = |extends: &Path, dependency: &Path| {
        fixture.write(
            "client/tsconfig.json",
            &format!(
                r#"{{"extends":{},"compilerOptions":{{"paths":{{"probe-sdk":[{}]}}}},"files":["main.ts"]}}"#,
                quoted(extends),
                quoted(dependency)
            ),
        )
    };
    let tsc = |config: &Path, cwd: &Path| {
        let mut command = Command::new(if cfg!(windows) { "pnpm.cmd" } else { "pnpm" });
        succeeds(command.current_dir(cwd).args(["exec", "tsc", "--project"]).arg(config));
    };
    tsc(&config(&sdk, &dependency), &fixture.0);
    let extends = if canonical_extends { canonical(&sdk) } else { sdk };
    let dependency = if canonical_paths {
        canonical(&dependency)
    } else {
        dependency
    };
    let cwd = if canonical_cwd {
        canonical(&fixture.0)
    } else {
        fixture.0.clone()
    };
    tsc(&config(&extends, &dependency), &cwd);
}

#[test]
fn typescript_canonical_tsconfig_extends() {
    typescript_probe(true, false, false);
}

#[test]
fn typescript_canonical_tsconfig_paths() {
    typescript_probe(false, true, false);
}

#[test]
fn typescript_pnpm_canonical_working_directory() {
    typescript_probe(false, false, true);
}

#[test]
fn cargo_canonical_path_dependency() {
    let fixture = Fixture::new();
    let dependency = fixture.write(
        "dependency/Cargo.toml",
        "[package]\nname = \"probe-dependency\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[workspace]\n",
    );
    fixture.write("dependency/src/lib.rs", "pub fn value() -> u32 { 42 }\n");
    fixture.write(
        "client/src/lib.rs",
        "pub fn value() -> u32 { probe_dependency::value() }\n",
    );
    for dependency in [
        dependency.parent().unwrap().to_path_buf(),
        canonical(dependency.parent().unwrap()),
    ] {
        let manifest = fixture.write(
            "client/Cargo.toml",
            &format!(
                "[package]\nname = \"probe-client\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[workspace]\n[dependencies]\nprobe-dependency = {{ path = {} }}\n",
                quoted(&dependency)
            ),
        );
        succeeds(
            Command::new("cargo")
                .current_dir(&fixture.0)
                .args(["check", "--offline", "--manifest-path"])
                .arg(manifest),
        );
    }
}
