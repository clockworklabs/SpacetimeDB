//! Builds non-Rust fixtures once, before servers or test processes are started.

use anyhow::{bail, ensure, Context, Result};
use regex::Regex;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::{build_typescript_sdk, csharp, have_emscripten, modules, pnpm, pnpm_path, workspace_root};

const HTTP_DOC: &str = "docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md";
const DOTNET_DISABLED: &str = ".dotnet-disabled";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Language {
    TypeScript,
    CSharp,
    Cpp,
}

impl Language {
    fn name(self) -> &'static str {
        match self {
            Self::TypeScript => "typescript",
            Self::CSharp => "csharp",
            Self::Cpp => "cpp",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::TypeScript => "js",
            Self::CSharp | Self::Cpp => "wasm",
        }
    }

    fn available(self) -> bool {
        match self {
            Self::TypeScript => pnpm_path().is_some(),
            Self::CSharp => Command::new("dotnet").arg("--list-sdks").output().is_ok_and(|output| {
                output.status.success()
                    && supports_csharp_fixtures(std::env::consts::OS, &String::from_utf8_lossy(&output.stdout))
            }),
            Self::Cpp => have_emscripten(),
        }
    }
}

fn supports_csharp_fixtures(host: &str, sdks: &str) -> bool {
    // These fixtures use .NET 10 NativeAOT, which the CLI does not support on macOS.
    host != "macos" && sdks.lines().any(|line| line.starts_with("10."))
}

// Tutorial fixtures are extracted from current documentation at preparation time.
const FIXTURES: &[(Language, &str, &str)] = &[
    (
        Language::Cpp,
        "http-routes-cpp-basic",
        include_str!("../modules/cpp/http-routes-cpp-basic.cpp"),
    ),
    (
        Language::Cpp,
        "http-routes-cpp-example",
        include_str!("../modules/cpp/http-routes-cpp-example.cpp"),
    ),
    (
        Language::Cpp,
        "http-routes-cpp-strict-non-root",
        include_str!("../modules/cpp/http-routes-cpp-strict-non-root.cpp"),
    ),
    (
        Language::Cpp,
        "http-routes-cpp-strict-root",
        include_str!("../modules/cpp/http-routes-cpp-strict-root.cpp"),
    ),
    (
        Language::Cpp,
        "http-routes-cpp-full-uri",
        include_str!("../modules/cpp/http-routes-cpp-full-uri.cpp"),
    ),
    (
        Language::Cpp,
        "http-routes-cpp-request-body",
        include_str!("../modules/cpp/http-routes-cpp-request-body.cpp"),
    ),
    (
        Language::TypeScript,
        "http-routes-typescript-basic",
        include_str!("../modules/typescript/http-routes-typescript-basic.ts"),
    ),
    (
        Language::TypeScript,
        "http-routes-typescript-example",
        include_str!("../modules/typescript/http-routes-typescript-example.ts"),
    ),
    (
        Language::TypeScript,
        "http-routes-typescript-strict-non-root",
        include_str!("../modules/typescript/http-routes-typescript-strict-non-root.ts"),
    ),
    (
        Language::TypeScript,
        "http-routes-typescript-strict-root",
        include_str!("../modules/typescript/http-routes-typescript-strict-root.ts"),
    ),
    (
        Language::TypeScript,
        "http-routes-typescript-full-uri",
        include_str!("../modules/typescript/http-routes-typescript-full-uri.ts"),
    ),
    (
        Language::TypeScript,
        "http-routes-typescript-request-body",
        include_str!("../modules/typescript/http-routes-typescript-request-body.ts"),
    ),
    (
        Language::CSharp,
        "http-routes-csharp-basic",
        include_str!("../modules/csharp/http-routes-csharp-basic.cs"),
    ),
    (
        Language::CSharp,
        "http-routes-csharp-example",
        include_str!("../modules/csharp/http-routes-csharp-example.cs"),
    ),
    (
        Language::CSharp,
        "http-routes-csharp-strict-non-root",
        include_str!("../modules/csharp/http-routes-csharp-strict-non-root.cs"),
    ),
    (
        Language::CSharp,
        "http-routes-csharp-strict-root",
        include_str!("../modules/csharp/http-routes-csharp-strict-root.cs"),
    ),
    (
        Language::CSharp,
        "http-routes-csharp-full-uri",
        include_str!("../modules/csharp/http-routes-csharp-full-uri.cs"),
    ),
    (
        Language::CSharp,
        "http-routes-csharp-request-body",
        include_str!("../modules/csharp/http-routes-csharp-request-body.cs"),
    ),
    (
        Language::TypeScript,
        "column-defaults-ts-initial",
        include_str!("../modules/typescript/column-defaults-ts-initial.ts"),
    ),
    (
        Language::TypeScript,
        "column-defaults-ts-updated",
        include_str!("../modules/typescript/column-defaults-ts-updated.ts"),
    ),
    (
        Language::CSharp,
        "column-defaults-csharp-initial",
        include_str!("../modules/csharp/column-defaults-csharp-initial.cs"),
    ),
    (
        Language::CSharp,
        "column-defaults-csharp-updated",
        include_str!("../modules/csharp/column-defaults-csharp-updated.cs"),
    ),
    (
        Language::Cpp,
        "column-defaults-cpp-initial",
        include_str!("../modules/cpp/column-defaults-cpp-initial.cpp"),
    ),
    (
        Language::Cpp,
        "column-defaults-cpp-updated",
        include_str!("../modules/cpp/column-defaults-cpp-updated.cpp"),
    ),
    (
        Language::TypeScript,
        "views-subscribe-typescript",
        include_str!("../modules/typescript/views-subscribe-typescript.ts"),
    ),
    (
        Language::CSharp,
        "views-count-csharp",
        include_str!("../modules/csharp/views-count-csharp.cs"),
    ),
    (
        Language::TypeScript,
        "views-count-typescript",
        include_str!("../modules/typescript/views-count-typescript.ts"),
    ),
    (
        Language::CSharp,
        "views-csharp",
        include_str!("../modules/csharp/views-csharp.cs"),
    ),
    (
        Language::TypeScript,
        "modules-basic-ts",
        include_str!("../modules/typescript/modules-basic-ts.ts"),
    ),
    (
        Language::TypeScript,
        "typescript-add-optional-columns-v1",
        include_str!("../modules/typescript/typescript-add-optional-columns-v1.ts"),
    ),
    (
        Language::TypeScript,
        "typescript-add-optional-columns-v2",
        include_str!("../modules/typescript/typescript-add-optional-columns-v2.ts"),
    ),
    (
        Language::TypeScript,
        "typescript-change-source-name-v2",
        include_str!("../modules/typescript/typescript-change-source-name-v2.ts"),
    ),
];

/// Preserve disabled C# support when an archive moves to another machine.
pub fn dotnet_prepared() -> bool {
    !modules::prepared_modules_dir().join(DOTNET_DISABLED).exists()
}

/// Builds enabled fixtures. CI requires every enabled toolchain; local filtered
/// runs may skip unavailable languages. Selecting a missing artifact still fails.
pub fn prepare_modules(cli: &Path, dotnet: bool, require_toolchains: bool) -> Result<()> {
    prepare_into(&modules::prepared_modules_dir(), |output| {
        if !dotnet {
            fs::write(output.join(DOTNET_DISABLED), [])?;
        }
        let project = tempfile::tempdir()?;
        let config = project.path().join("config.toml");
        for language in [Language::TypeScript, Language::CSharp, Language::Cpp] {
            if language == Language::CSharp && !dotnet {
                continue;
            }
            if !language.available() {
                ensure!(
                    !require_toolchains,
                    "Missing toolchain for {} fixtures",
                    language.name()
                );
                eprintln!("Skipping {} fixtures: toolchain unavailable", language.name());
                continue;
            }
            if language == Language::TypeScript {
                build_typescript_sdk()?;
            }
            for &(fixture_language, name, source) in FIXTURES {
                if language == fixture_language {
                    build_fixture(cli, &config, project.path(), output, language, name, source)
                        .with_context(|| format!("preparing fixture {name}"))?;
                }
            }
            let doc = fs::read_to_string(workspace_root().join(HTTP_DOC))?;
            let source = tutorial_source(&doc, language)?;
            let name = format!("http-handlers-docs-{}", language.name());
            build_fixture(cli, &config, project.path(), output, language, &name, &source)
                .with_context(|| format!("preparing fixture {name}"))?;
        }
        Ok(())
    })
}

// Invalidate previous artifacts first, and expose the new set only after success.
fn prepare_into(destination: &Path, build: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    if destination.exists() {
        fs::remove_dir_all(destination)?;
    }
    let parent = destination.parent().context("Missing preparation output parent")?;
    fs::create_dir_all(parent)?;
    let staging = tempfile::tempdir_in(parent)?;
    build(staging.path())?;
    fs::rename(staging.path(), destination)?;
    Ok(())
}

fn tutorial_source(doc: &str, language: Language) -> Result<String> {
    let language_pattern = match language {
        Language::TypeScript => "(?:ts|typescript)",
        Language::CSharp => "csharp",
        Language::Cpp => r"(?:cpp|c\+\+)",
    };
    let doc = doc.replace("\r\n", "\n");
    let regex = Regex::new(&format!(r"```{language_pattern}\n([\s\S]*?)\n```"))?;
    let blocks = regex
        .captures_iter(&doc)
        .map(|cap| cap[1].to_string())
        .collect::<Vec<_>>();
    ensure!(!blocks.is_empty(), "No {} code blocks in {HTTP_DOC}", language.name());
    Ok(blocks.join("\n\n"))
}

fn spacetime(cli: &Path, config: &Path, cwd: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new(cli)
        .arg("--config-path")
        .arg(config)
        .args(args)
        .current_dir(cwd)
        .output()?;
    if !output.status.success() {
        bail!(
            "spacetime {args:?} failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn build_fixture(
    cli: &Path,
    config: &Path,
    project: &Path,
    output: &Path,
    language: Language,
    name: &str,
    source: &str,
) -> Result<()> {
    eprintln!("Building precompiled fixture {name}...");
    let root = project.join(name);
    let module = if language == Language::Cpp {
        fs::create_dir_all(root.join("src"))?;
        let bindings = workspace_root()
            .join("crates/bindings-cpp")
            .display()
            .to_string()
            .replace('\\', "/");
        let cmake = include_str!("../modules/cpp/CMakeLists.txt").replace("@SPACETIMEDB_CPP_LIBRARY_PATH@", &bindings);
        fs::write(root.join("CMakeLists.txt"), cmake)?;
        fs::write(root.join("src/lib.cpp"), source)?;
        root
    } else {
        let mut args = vec![
            "init",
            "--non-interactive",
            "--lang",
            language.name(),
            "--project-path",
            root.to_str().context("Invalid fixture path")?,
            name,
        ];
        if language == Language::CSharp {
            args.extend(["--dotnet-version", "10"]);
        }
        spacetime(cli, config, project, &args)?;
        let module = root.join("spacetimedb");
        if language == Language::TypeScript {
            fs::write(module.join("src/index.ts"), source)?;
            let _ = pnpm(&["uninstall", "spacetimedb"], &module);
            let bindings = workspace_root().join("crates/bindings-typescript");
            pnpm(
                &[
                    "install",
                    bindings.to_str().context("Invalid TypeScript bindings path")?,
                ],
                &module,
            )?;
        } else {
            fs::write(module.join("Lib.cs"), source)?;
            csharp::prepare_csharp_module(&module)?;
        }
        module
    };
    let mut args = vec![
        "build",
        "--module-path",
        module.to_str().context("Invalid module path")?,
    ];
    if language == Language::CSharp {
        args.extend(["--dotnet-version", "10"]);
    }
    spacetime(cli, config, project, &args)?;
    if language == Language::CSharp {
        csharp::verify_csharp_module_restore(&module)?;
    }
    let candidates: &[&str] = match language {
        Language::TypeScript => &["dist/bundle.js"],
        Language::CSharp => &[
            "bin/Release/net10.0/wasi-wasm/native/StdbModule.wasm",
            "bin/Release/net10.0/native/StdbModule.wasm",
        ],
        Language::Cpp => &["build/lib.wasm", "build/Release/lib.wasm"],
    };
    let artifacts = candidates
        .iter()
        .map(|path| module.join(path))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    ensure!(
        artifacts.len() == 1,
        "Expected one artifact for {name}, found {artifacts:?}"
    );
    fs::copy(
        &artifacts[0],
        output.join(format!(
            "smoketest_module_{}.{}",
            name.replace('-', "_"),
            language.extension()
        )),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csharp_preparation_requires_a_supported_dotnet_10_host() {
        assert!(!supports_csharp_fixtures("linux", "8.0.100 [/sdk]\n9.0.100 [/sdk]"));
        assert!(supports_csharp_fixtures("linux", "8.0.100 [/sdk]\n10.0.100 [/sdk]"));
        assert!(supports_csharp_fixtures("windows", "10.0.100 [C:\\sdk]\r\n"));
        assert!(!supports_csharp_fixtures("macos", "10.0.100 [/sdk]"));
    }

    #[test]
    fn failed_preparation_invalidates_previous_and_partial_artifacts() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("prepared");
        fs::create_dir(&dest).unwrap();
        fs::write(dest.join("stale.wasm"), []).unwrap();
        let result = prepare_into(&dest, |staging| {
            fs::write(staging.join("partial.wasm"), [])?;
            bail!("build failed")
        });
        assert!(result.is_err());
        assert!(!dest.exists());
        prepare_into(&dest, |staging| {
            fs::write(staging.join(DOTNET_DISABLED), [])?;
            Ok(())
        })
        .unwrap();
        assert!(dest.join(DOTNET_DISABLED).exists());
        assert!(!dest.join("stale.wasm").exists());
    }

    #[test]
    fn tutorial_uses_current_document_and_requires_matching_blocks() {
        let doc = "```typescript\r\nfirst\r\n```\r\n```ts\r\nsecond\r\n```";
        assert_eq!(tutorial_source(doc, Language::TypeScript).unwrap(), "first\n\nsecond");
        assert_eq!(
            tutorial_source(&doc.replace("first", "changed"), Language::TypeScript).unwrap(),
            "changed\n\nsecond"
        );
        assert!(tutorial_source(doc, Language::Cpp).is_err());
    }

    #[test]
    fn fixture_names_are_unique() {
        let mut names = std::collections::HashSet::new();
        for &(_, name, _) in FIXTURES {
            assert!(names.insert(name.replace('_', "-")), "Duplicate fixture {name}");
        }
    }
}
