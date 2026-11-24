#![allow(clippy::disallowed_macros)]

use anyhow::{bail, ensure, Context, Result};
use clap::{ArgAction, Parser, Subcommand};
use duct::cmd;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Builds, packs and checks the SpacetimeDB.Testing NuGet packages.
///
/// `.github/workflows/csharp-testing.yml` runs one subcommand per job, through
/// `cargo ci other-workflows csharp-testing-packages`. Each one also runs locally;
/// outputs go to `$GITHUB_OUTPUT` when it is set and to stdout otherwise.
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Resolve the commit to build, the package version, and the platform matrix.
    Prepare {
        /// Release tag, such as v2.1.0. Sets the package version.
        #[arg(long, env = "RELEASE_TAG")]
        release_tag: Option<String>,
        /// Build the release tag's commit, after checking that it is in protected master or release/* history.
        #[arg(long, action = ArgAction::Set, default_value_t = false)]
        use_release_tag: bool,
        /// Every supported platform; otherwise linux-x64 only.
        #[arg(long, action = ArgAction::Set, default_value_t = false)]
        all_platforms: bool,
    },
    /// Build the native bridge for the host, check its linkage, and run the managed tests against it.
    Native {
        #[arg(long)]
        rid: String,
        /// Staging directory; the library is written to <OUT>/<RID>/.
        #[arg(long, default_value = "native")]
        out: PathBuf,
    },
    /// Pack BSATN.Runtime, Runtime and Testing, and check the Testing package's contents.
    Pack {
        #[arg(long)]
        version: String,
        /// Staging directory of tested native libraries, as written by `native`.
        #[arg(long, default_value = "native")]
        native_dir: PathBuf,
        #[arg(long, default_value = "feed")]
        out: PathBuf,
        /// Require every platform. Otherwise pack a prerelease for the platforms in --native-dir.
        #[arg(long, action = ArgAction::Set, default_value_t = false)]
        all_platforms: bool,
    },
    /// Restore the packages from a feed into a copy of the chat-console-cs template and run its tests.
    Accept {
        #[arg(long)]
        version: String,
        #[arg(long, default_value = "feed")]
        feed: PathBuf,
        #[arg(long)]
        rid: String,
    },
}

struct Platform {
    rid: &'static str,
    target: &'static str,
    runner: &'static str,
    library: &'static str,
}

// Match the CLI's glibc baseline. Do not upgrade these images implicitly.
// Release packs require this exact order; see TestingNativeRids in Testing.csproj.
const PLATFORMS: &[Platform] = &[
    Platform {
        rid: "linux-x64",
        target: "x86_64-unknown-linux-gnu",
        runner: "ubuntu-22.04",
        library: "libspacetimedb_test_datastore_ffi.so",
    },
    Platform {
        rid: "linux-arm64",
        target: "aarch64-unknown-linux-gnu",
        runner: "ubuntu-22.04-arm",
        library: "libspacetimedb_test_datastore_ffi.so",
    },
    Platform {
        rid: "win-x64",
        target: "x86_64-pc-windows-msvc",
        runner: "windows-2022",
        library: "spacetimedb_test_datastore_ffi.dll",
    },
];

/// The newest glibc symbol version the Linux bridge may require: Ubuntu 22.04's.
const MAX_GLIBC: (u32, u32) = (2, 35);

fn main() -> Result<()> {
    ci_common::ensure_repo_root()?;
    match Cli::parse().command {
        Command::Prepare {
            release_tag,
            use_release_tag,
            all_platforms,
        } => prepare(
            // Workflow inputs pass an unset tag as "".
            release_tag.as_deref().filter(|tag| !tag.is_empty()),
            use_release_tag,
            all_platforms,
        ),
        Command::Native { rid, out } => native(platform(&rid)?, &out),
        Command::Pack {
            version,
            native_dir,
            out,
            all_platforms,
        } => pack(&version, &native_dir, &out, all_platforms),
        Command::Accept { version, feed, rid } => accept(&version, &feed, platform(&rid)?),
    }
}

fn platform(rid: &str) -> Result<&'static Platform> {
    PLATFORMS
        .iter()
        .find(|platform| platform.rid == rid)
        .with_context(|| format!("Unsupported RID {rid}"))
}

fn set_output(name: &str, value: &str) -> Result<()> {
    println!("{name}={value}");
    if let Some(path) = std::env::var_os("GITHUB_OUTPUT") {
        let mut file = OpenOptions::new().append(true).open(path)?;
        writeln!(file, "{name}={value}")?;
    }
    Ok(())
}

fn prepare(release_tag: Option<&str>, use_release_tag: bool, all_platforms: bool) -> Result<()> {
    let sha = match (release_tag, use_release_tag) {
        (Some(tag), true) => trusted_tag_commit(tag)?,
        (None, true) => bail!("--use-release-tag true requires --release-tag"),
        (_, false) => cmd!("git", "rev-parse", "HEAD").read()?,
    };
    set_output("sha", &sha)?;

    let version = match release_tag {
        Some(tag) => version_from_tag(tag)?,
        None => {
            let run = match (std::env::var("GITHUB_RUN_ID"), std::env::var("GITHUB_RUN_ATTEMPT")) {
                (Ok(id), Ok(attempt)) => format!("{id}.{attempt}"),
                _ => format!("local.{}", SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()),
            };
            format!("0.0.0-packagetest.{run}")
        }
    };
    set_output("package-version", &version)?;

    let platforms = if all_platforms { PLATFORMS } else { &PLATFORMS[..1] };
    let include = platforms
        .iter()
        .map(|platform| format!(r#"{{"rid":"{}","runner":"{}"}}"#, platform.rid, platform.runner))
        .collect::<Vec<_>>()
        .join(",");
    set_output("platforms", &format!(r#"{{"include":[{include}]}}"#))
}

fn validate_release_tag(tag: &str) -> Result<()> {
    let pattern = regex::Regex::new(r"^v[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$")?;
    ensure!(pattern.is_match(tag), "Invalid release tag: {tag}");
    Ok(())
}

fn version_from_tag(tag: &str) -> Result<String> {
    validate_release_tag(tag)?;
    // Hotfix tags reuse the existing NuGet version, as in cargo-release.
    Ok(tag[1..].split("-hotfix").next().unwrap_or_default().to_owned())
}

fn trusted_tag_commit(tag: &str) -> Result<String> {
    validate_release_tag(tag)?;
    let tag_ref = format!("refs/tags/{tag}");
    cmd!("git", "check-ref-format", &tag_ref).run()?;
    cmd!("git", "fetch", "--no-tags", "origin", &tag_ref).run()?;
    let sha = cmd!("git", "rev-parse", "--verify", "FETCH_HEAD^{commit}").read()?;

    // A tag name alone is not a trust boundary. Require protected branch history.
    let repo = std::env::var("GITHUB_REPOSITORY").unwrap_or_else(|_| "clockworklabs/SpacetimeDB".to_owned());
    let heads = cmd!(
        "gh",
        "api",
        "--paginate",
        format!("repos/{repo}/branches?protected=true&per_page=100"),
        "--jq",
        r#".[] | select(.name == "master" or (.name | startswith("release/"))) | .commit.sha"#
    )
    .read()?;
    for head in heads.lines().filter(|head| !head.is_empty()) {
        cmd!("git", "fetch", "--no-tags", "origin", head).run()?;
        let is_ancestor = cmd!("git", "merge-base", "--is-ancestor", &sha, head)
            .unchecked()
            .run()?
            .status
            .success();
        if is_ancestor {
            return Ok(sha);
        }
    }
    bail!("Release tag {tag} must belong to protected master or release/* history")
}

fn native(platform: &Platform, out: &Path) -> Result<()> {
    // A cross-compiled binary alone is not platform coverage.
    let host = cmd!("rustc", "-vV").read()?;
    ensure!(
        host.lines().any(|line| line == format!("host: {}", platform.target)),
        "{} must be built on a {} host",
        platform.rid,
        platform.target
    );
    cmd!(
        "cargo",
        "build",
        "--locked",
        "--release",
        "--target",
        platform.target,
        "-p",
        "spacetimedb-test-datastore-ffi"
    )
    .run()?;

    let target_dir = std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| PathBuf::from("target"), PathBuf::from);
    let built = target_dir.join(platform.target).join("release").join(platform.library);
    let staged_dir = out.join(platform.rid);
    fs::create_dir_all(&staged_dir)?;
    let library = std::path::absolute(staged_dir.join(platform.library))?;
    fs::copy(&built, &library).with_context(|| format!("Failed to copy {}", built.display()))?;

    if platform.rid.starts_with("linux-") {
        check_linux_linkage(&library)?;
    }

    cmd!(
        "dotnet",
        "test",
        "crates/bindings-csharp/Testing.Tests/Testing.Tests.csproj",
        "-c",
        "Release",
        "-warnaserror",
        format!("-p:NativeTestLibrary={}", library.display())
    )
    .run()?;
    Ok(())
}

fn check_linux_linkage(library: &Path) -> Result<()> {
    let dependencies = cmd!("ldd", library).read()?;
    println!("{dependencies}");
    ensure!(
        !dependencies.contains("not found"),
        "{} has unresolved dependencies",
        library.display()
    );

    // Reject accidental linkage against a newer glibc than Ubuntu 22.04's.
    let versions = cmd!("readelf", "--version-info", library).read()?;
    let required = max_glibc(&versions).context("Found no GLIBC symbol versions")?;
    println!("Requires GLIBC_{}.{}", required.0, required.1);
    ensure!(
        required <= MAX_GLIBC,
        "{} requires GLIBC_{}.{}, newer than GLIBC_{}.{}",
        library.display(),
        required.0,
        required.1,
        MAX_GLIBC.0,
        MAX_GLIBC.1
    );
    Ok(())
}

fn max_glibc(version_info: &str) -> Option<(u32, u32)> {
    let pattern = regex::Regex::new(r"GLIBC_([0-9]+)\.([0-9]+)").expect("valid regex");
    pattern
        .captures_iter(version_info)
        .filter_map(|captures| Some((captures[1].parse().ok()?, captures[2].parse().ok()?)))
        .max()
}

fn pack(version: &str, native_dir: &Path, out: &Path, all_platforms: bool) -> Result<()> {
    for project in ["BSATN.Runtime", "Runtime"] {
        cmd!(
            "dotnet",
            "pack",
            format!("crates/bindings-csharp/{project}"),
            "-c",
            "Release",
            format!("-p:Version={version}"),
            "-o",
            out
        )
        .run()?;
    }

    let staged = PLATFORMS
        .iter()
        .filter(|platform| native_dir.join(platform.rid).join(platform.library).is_file())
        .collect::<Vec<_>>();
    ensure!(!staged.is_empty(), "No native libraries in {}", native_dir.display());
    let rids = staged.iter().map(|platform| platform.rid).collect::<Vec<_>>().join(";");
    // Testing.csproj rejects a release package that lacks a platform. Pull-request packages are
    // prereleases for the platforms tested.
    cmd!(
        "dotnet",
        "pack",
        "crates/bindings-csharp/Testing",
        "-c",
        "Release",
        format!("-p:Version={version}"),
        "-o",
        out,
        format!(
            "-p:TestingNativeArtifactsDir={}",
            std::path::absolute(native_dir)?.display()
        ),
        format!("-p:TestingNativeRids={rids}"),
        format!("-p:TestingLocalPackage={}", !all_platforms)
    )
    .run()?;

    let package = out.join(format!("SpacetimeDB.Testing.{version}.nupkg"));
    let mut archive =
        zip::ZipArchive::new(File::open(&package)?).with_context(|| format!("Failed to open {}", package.display()))?;

    // Check that packing preserved every tested binary at its NuGet runtime path.
    for platform in staged {
        let entry = format!("runtimes/{}/native/{}", platform.rid, platform.library);
        let packed = read_entry(&mut archive, &entry)?;
        let tested = fs::read(native_dir.join(platform.rid).join(platform.library))?;
        ensure!(packed == tested, "{entry} differs from the tested library");
    }

    // A minimum-version dependency would let a different Runtime satisfy Testing.
    let nuspec = String::from_utf8(read_entry(&mut archive, "SpacetimeDB.Testing.nuspec")?)?;
    println!("{nuspec}");
    ensure!(
        nuspec.contains(&format!(r#"id="SpacetimeDB.Runtime" version="[{version}]""#)),
        "SpacetimeDB.Testing must pin SpacetimeDB.Runtime to exactly [{version}]"
    );
    Ok(())
}

fn read_entry(archive: &mut zip::ZipArchive<File>, name: &str) -> Result<Vec<u8>> {
    let mut entry = archive
        .by_name(name)
        .with_context(|| format!("Package has no {name}"))?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn accept(version: &str, feed: &Path, platform: &Platform) -> Result<()> {
    let feed = std::path::absolute(feed)?;
    let temp_dir = tempfile::tempdir()?;
    let work = temp_dir.path();
    let template = Path::new("templates/chat-console-cs");

    fs::create_dir_all(work.join("spacetimedb"))?;
    fs::copy(template.join("spacetimedb/Lib.cs"), work.join("spacetimedb/Lib.cs"))?;
    // Match the .NET 10 project emitted by spacetime init.
    let csproj = fs::read_to_string(template.join("spacetimedb/StdbModule.csproj"))?;
    let multi_target = "<TargetFrameworks>net8.0;net10.0</TargetFrameworks>";
    ensure!(
        csproj.contains(multi_target),
        "Template StdbModule.csproj no longer has {multi_target}"
    );
    fs::write(
        work.join("spacetimedb/StdbModule.csproj"),
        csproj.replace(multi_target, "<TargetFramework>net10.0</TargetFramework>"),
    )?;

    fs::create_dir_all(work.join("tests"))?;
    for entry in fs::read_dir(template.join("tests"))? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            fs::copy(entry.path(), work.join("tests").join(entry.file_name()))?;
        }
    }
    fs::copy("global.json", work.join("global.json"))?;

    fs::write(
        work.join("Directory.Build.props"),
        r#"<Project>
  <PropertyGroup>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
  </PropertyGroup>
</Project>
"#,
    )?;
    fs::write(
        work.join("Directory.Build.targets"),
        format!(
            r#"<Project>
  <ItemGroup>
    <PackageReference Update="SpacetimeDB.Runtime;SpacetimeDB.Testing" Version="[{version}]" />
  </ItemGroup>
</Project>
"#
        ),
    )?;
    fs::write(
        work.join("NuGet.Config"),
        format!(
            r#"<configuration>
  <packageSources>
    <clear />
    <add key="local" value="{}" />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
  </packageSources>
  <packageSourceMapping>
    <clear />
    <packageSource key="local"><package pattern="SpacetimeDB.*" /></packageSource>
    <packageSource key="nuget.org"><package pattern="*" /></packageSource>
  </packageSourceMapping>
</configuration>
"#,
            feed.display()
        ),
    )?;

    let dotnet = |args: &[&str]| {
        cmd("dotnet", args)
            .dir(work)
            .env("NUGET_PACKAGES", work.join("packages"))
            .env("NUGET_HTTP_CACHE_PATH", work.join("http-cache"))
            .run()
    };
    dotnet(&["restore", "tests/Module.Tests.csproj", "--configfile", "NuGet.Config"])?;
    dotnet(&[
        "test",
        "tests/Module.Tests.csproj",
        "--no-restore",
        "-c",
        "Release",
        "-warnaserror",
    ])?;

    let library = work
        .join("tests/bin/Release/net10.0/runtimes")
        .join(platform.rid)
        .join("native")
        .join(platform.library);
    ensure!(
        library.is_file(),
        "The test output has no native bridge at {}",
        library.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_come_from_release_tags() -> Result<()> {
        assert_eq!(version_from_tag("v2.1.0")?, "2.1.0");
        assert_eq!(version_from_tag("v2.1.0-rc.1")?, "2.1.0-rc.1");
        assert_eq!(version_from_tag("v2.1.0-hotfix1")?, "2.1.0");
        assert_eq!(version_from_tag("v2.1.0-hotfix.2")?, "2.1.0");
        Ok(())
    }

    #[test]
    fn rejects_malformed_release_tags() {
        for tag in ["2.1.0", "v2.1", "v2.1.0;rm", "v2.1.0 ", "refs/tags/v2.1.0", ""] {
            assert!(validate_release_tag(tag).is_err(), "accepted {tag:?}");
        }
    }

    #[test]
    fn finds_newest_glibc_numerically() {
        let info = "GLIBC_2.2.5 GLIBC_2.17 GLIBC_2.34 GLIBC_2.9 GLIBC_PRIVATE";
        assert_eq!(max_glibc(info), Some((2, 34)));
        assert_eq!(max_glibc("GLIBC_2.36"), Some((2, 36)));
        assert_eq!(max_glibc("nothing here"), None);
    }

    #[test]
    fn release_platform_order_matches_testing_csproj() {
        let rids = PLATFORMS.iter().map(|platform| platform.rid).collect::<Vec<_>>();
        assert_eq!(rids.join(";"), "linux-x64;linux-arm64;win-x64");
    }
}
