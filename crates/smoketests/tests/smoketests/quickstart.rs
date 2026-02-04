#![allow(clippy::disallowed_macros)]
//! This test validates that the quickstart documentation is correct by extracting
//! code from markdown docs and running it.

use anyhow::{bail, Context, Result};
use chrono::Local;
use regex::Regex;
use spacetimedb_smoketests::{pnpm_path, require_dotnet, require_pnpm, workspace_root, Smoketest};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Write content to a file, creating parent directories as needed.
fn write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}

/// Append content to a file.
fn append_to_file(path: &Path, content: &str) -> Result<()> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new().append(true).open(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

/// Run a command and return stdout as a string.
fn run_cmd(args: &[&str], cwd: &Path, input: Option<&str>) -> Result<String> {
    let mut cmd = Command::new(args[0]);
    cmd.args(&args[1..])
        .current_dir(cwd)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped());

    if input.is_some() {
        cmd.stdin(Stdio::piped());
    }

    let mut child = cmd.spawn().context(format!("Failed to spawn {:?}", args))?;

    if let Some(input_str) = input {
        use std::io::Write;
        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(input_str.as_bytes())?;
        }
    }

    let output = child.wait_with_output()?;

    if !output.status.success() {
        bail!(
            "Command {:?} failed:\nstdout: {}\nstderr: {}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Parse code blocks from quickstart markdown documentation.
/// Extracts code blocks with the specified language tag.
///
/// - `language`: "rust", "csharp", or "typescript"
/// - `module_name`: The name to replace "quickstart-chat" with
/// - `server`: If true, look for server code blocks (e.g. "rust server"), else client blocks
fn parse_quickstart(doc_content: &str, language: &str, module_name: &str, server: bool) -> String {
    // Normalize line endings to Unix style (LF) for consistent regex matching
    let doc_content = doc_content.replace("\r\n", "\n");

    // Determine the codeblock language tag to search for
    let codeblock_lang = if server {
        if language == "typescript" {
            "ts server".to_string()
        } else {
            format!("{} server", language)
        }
    } else if language == "typescript" {
        "ts".to_string()
    } else {
        language.to_string()
    };

    // Extract code blocks with the specified language
    let pattern = format!(r"```{}\n([\s\S]*?)\n```", regex::escape(&codeblock_lang));
    let re = Regex::new(&pattern).unwrap();
    let mut blocks: Vec<String> = re
        .captures_iter(&doc_content)
        .map(|cap| cap.get(1).unwrap().as_str().to_string())
        .collect();

    let mut end = String::new();

    // C# specific fixups
    if language == "csharp" {
        let mut found_on_connected = false;
        let mut filtered_blocks = Vec::new();

        for mut block in blocks {
            // The doc first creates an empty class Module, so we need to fixup the closing brace
            if block.contains("partial class Module") {
                block = block.replace("}", "");
                end = "\n}".to_string();
            }
            // Remove the first `OnConnected` block, which body is later updated
            if block.contains("OnConnected(DbConnection conn") && !found_on_connected {
                found_on_connected = true;
                continue;
            }
            filtered_blocks.push(block);
        }
        blocks = filtered_blocks;
    }

    // Join blocks and replace module name
    let result = blocks.join("\n").replace("quickstart-chat", module_name);
    result + &end
}

/// Run pnpm command.
fn pnpm(args: &[&str], cwd: &Path) -> Result<String> {
    let pnpm_path = match pnpm_path()
        .expect("Could not locate pnpm")
        .into_os_string()
        .into_string()
    {
        Ok(s) => s,
        Err(os_string) => anyhow::bail!("Could not convert to string: {os_string:?}"),
    };
    let mut full_args = vec![pnpm_path.as_ref()];
    full_args.extend(args);
    run_cmd(&full_args, cwd, None)
}

/// Build the TypeScript SDK.
fn build_typescript_sdk() -> Result<()> {
    let workspace = workspace_root();
    let ts_bindings = workspace.join("crates/bindings-typescript");
    pnpm(&["install"], &ts_bindings)?;
    pnpm(&["build"], &ts_bindings)?;
    Ok(())
}

fn nuget_config_path(project_dir: &Path) -> PathBuf {
    let p_upper = project_dir.join("NuGet.Config");
    if p_upper.exists() {
        return p_upper;
    }

    let p_lower = project_dir.join("nuget.config");
    if p_lower.exists() {
        return p_lower;
    }

    p_upper
}

/// Create a NuGet config.
fn create_nuget_config(sources: &[(String, PathBuf)], mappings: &[(String, String)]) -> String {
    let mut source_lines = String::new();
    let mut mapping_lines = String::new();

    for (key, path) in sources {
        source_lines.push_str(&format!("    <add key=\"{}\" value=\"{}\" />\n", key, path.display()));
    }

    for (key, pattern) in mappings {
        mapping_lines.push_str(&format!(
            "    <packageSource key=\"{}\">\n      <package pattern=\"{}\" />\n    </packageSource>\n",
            key, pattern
        ));
    }

    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
{}  </packageSources>
  <packageSourceMapping>
{}  </packageSourceMapping>
</configuration>
"#,
        source_lines, mapping_lines
    )
}

/// Override nuget config to use a local NuGet package on a .NET project.
fn override_nuget_package(project_dir: &Path, package: &str, source_dir: &Path, build_subdir: &str) -> Result<()> {
    eprintln!("Override {package}: {project_dir:?} with {source_dir:?}");

    // Make sure the local package is built
    let workspace = workspace_root();
    let repo_nuget_config = workspace.join("NuGet.Config");
    if repo_nuget_config.exists() {
        println!("repo_nuget_config exists");
        let output = Command::new("dotnet")
            .args(["restore", "--configfile", repo_nuget_config.to_str().unwrap()])
            .current_dir(source_dir)
            .output()
            .context("Failed to run dotnet restore")?;
        if !output.status.success() {
            bail!(
                "dotnet restore failed:\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let output = Command::new("dotnet")
            .args(["pack", "-c", "Release", "--no-restore"])
            .current_dir(source_dir)
            .output()
            .context("Failed to run dotnet pack")?;
        if !output.status.success() {
            bail!(
                "dotnet pack failed:\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    } else {
        println!("repo_nuget_config does not exist");
        let output = Command::new("dotnet")
            .args(["pack", "-c", "Release"])
            .current_dir(source_dir)
            .output()
            .context("Failed to run dotnet pack")?;
        if !output.status.success() {
            bail!(
                "dotnet pack failed:\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    let nuget_config_path = nuget_config_path(project_dir);
    let source_dir = std::fs::canonicalize(source_dir).unwrap_or_else(|_| source_dir.to_path_buf());
    let package_path = source_dir.join(build_subdir);

    // Read existing config or create new one
    let (mut sources, mut mappings) = if nuget_config_path.exists() {
        // Parse existing config - simplified approach
        let content = fs::read_to_string(&nuget_config_path)?;
        parse_nuget_config(&content)
    } else {
        (Vec::new(), Vec::new())
    };

    // Add new source only if not already present (avoid duplicates)
    if !sources.iter().any(|(k, _)| k == package) {
        sources.push((package.to_string(), package_path));
    }

    // Add mapping for the package only if not already present
    if !mappings.iter().any(|(k, _)| k == package) {
        mappings.push((package.to_string(), package.to_string()));
    }

    // Ensure nuget.org fallback exists
    if !sources.iter().any(|(k, _)| k == "nuget.org") {
        sources.push((
            "nuget.org".to_string(),
            PathBuf::from("https://api.nuget.org/v3/index.json"),
        ));
    }
    if !mappings.iter().any(|(k, _)| k == "nuget.org") {
        mappings.push(("nuget.org".to_string(), "*".to_string()));
    }

    // Write config
    let config = create_nuget_config(&sources, &mappings);
    fs::write(&nuget_config_path, config)?;

    let _ = Command::new("dotnet")
        .args(["nuget", "locals", "--clear", "all"])
        .stderr(Stdio::null())
        .stdout(Stdio::null())
        .status();

    Ok(())
}

/// Parse an existing nuget.config file (simplified).
#[allow(clippy::type_complexity)]
fn parse_nuget_config(content: &str) -> (Vec<(String, PathBuf)>, Vec<(String, String)>) {
    let mut sources = Vec::new();
    let mut mappings = Vec::new();

    // Simple regex-based parsing
    let source_re = regex::Regex::new(r#"<add key="([^"]+)" value="([^"]+)""#).unwrap();
    for cap in source_re.captures_iter(content) {
        sources.push((cap[1].to_string(), PathBuf::from(&cap[2])));
    }

    let mapping_re = regex::Regex::new(r#"<packageSource key="([^"]+)">\s*<package pattern="([^"]+)""#).unwrap();
    for cap in mapping_re.captures_iter(content) {
        mappings.push((cap[1].to_string(), cap[2].to_string()));
    }

    (sources, mappings)
}

/// Quickstart test configuration.
struct QuickstartConfig {
    lang: &'static str,
    client_lang: &'static str,
    server_file: &'static str,
    client_file: &'static str,
    module_bindings: &'static str,
    run_cmd: &'static [&'static str],
    build_cmd: &'static [&'static str],
    replacements: &'static [(&'static str, &'static str)],
    extra_code: &'static str,
    connected_str: &'static str,
}

impl QuickstartConfig {
    fn rust() -> Self {
        Self {
            lang: "rust",
            client_lang: "rust",
            server_file: "src/lib.rs",
            client_file: "src/main.rs",
            module_bindings: "src/module_bindings",
            run_cmd: &["cargo", "run"],
            build_cmd: &["cargo", "build"],
            replacements: &[
                // Replace the interactive user input to allow direct testing
                ("user_input_loop(&ctx)", "user_input_direct(&ctx)"),
                // Don't cache the token, because it will cause the test to fail if we run against a non-default server
                (".with_token(creds_store()", "//.with_token(creds_store()"),
            ],
            extra_code: r#"
fn user_input_direct(ctx: &DbConnection) {
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).expect("Failed to read from stdin.");
    if let Some(name) = line.strip_prefix("/name ") {
        ctx.reducers.set_name(name.to_string()).unwrap();
    } else {
        ctx.reducers.send_message(line).unwrap();
    }
    std::thread::sleep(std::time::Duration::from_secs(1));
    std::process::exit(0);
}
"#,
            connected_str: "connected",
        }
    }

    fn csharp() -> Self {
        Self {
            lang: "csharp",
            client_lang: "csharp",
            server_file: "Lib.cs",
            client_file: "Program.cs",
            module_bindings: "module_bindings",
            run_cmd: &["dotnet", "run"],
            build_cmd: &["dotnet", "build"],
            replacements: &[
                // Replace the interactive user input to allow direct testing
                ("InputLoop();", "UserInputDirect();"),
                (".OnConnect(OnConnected)", ".OnConnect(OnConnectedSignal)"),
                (
                    ".OnConnectError(OnConnectError)",
                    ".OnConnectError(OnConnectErrorSignal)",
                ),
                // Don't cache the token
                (".WithToken(AuthToken.Token)", "//.WithToken(AuthToken.Token)"),
                // To put the main function at the end so it can see the new functions
                ("Main();", ""),
            ],
            extra_code: r#"
var connectedEvent = new ManualResetEventSlim(false);
var connectionFailed = new ManualResetEventSlim(false);
void OnConnectErrorSignal(Exception e)
{
     OnConnectError(e);
     connectionFailed.Set();
}
void OnConnectedSignal(DbConnection conn, Identity identity, string authToken)
{
    OnConnected(conn, identity, authToken);
    connectedEvent.Set();
}

void UserInputDirect() {
    string? line = Console.In.ReadToEnd()?.Trim();
    if (line == null) Environment.Exit(0);

    if (!WaitHandle.WaitAny(
            new[] { connectedEvent.WaitHandle, connectionFailed.WaitHandle },
            TimeSpan.FromSeconds(5)
        ).Equals(0))
    {
        Console.WriteLine("Failed to connect to server within timeout.");
        Environment.Exit(1);
    }

    if (line.StartsWith("/name ")) {
        input_queue.Enqueue(("name", line[6..]));
    } else {
        input_queue.Enqueue(("message", line));
    }
    Thread.Sleep(1000);
}
Main();
"#,
            connected_str: "Connected",
        }
    }

    fn typescript() -> Self {
        // TypeScript server uses Rust client because the TypeScript client
        // quickstart is a React app, which is difficult to smoketest.
        Self {
            lang: "typescript",
            client_lang: "rust",
            server_file: "src/index.ts",
            // Client uses Rust config
            client_file: "src/main.rs",
            module_bindings: "src/module_bindings",
            run_cmd: &["cargo", "run"],
            build_cmd: &["cargo", "build"],
            replacements: &[
                ("user_input_loop(&ctx)", "user_input_direct(&ctx)"),
                (".with_token(creds_store()", "//.with_token(creds_store()"),
            ],
            extra_code: r#"
fn user_input_direct(ctx: &DbConnection) {
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).expect("Failed to read from stdin.");
    if let Some(name) = line.strip_prefix("/name ") {
        ctx.reducers.set_name(name.to_string()).unwrap();
    } else {
        ctx.reducers.send_message(line).unwrap();
    }
    std::thread::sleep(std::time::Duration::from_secs(1));
    std::process::exit(0);
}
"#,
            connected_str: "connected",
        }
    }
}

/// Quickstart test runner.
struct QuickstartTest {
    test: Smoketest,
    config: QuickstartConfig,
    project_path: PathBuf,
    /// Temp directory for server/client - kept alive for duration of test
    _temp_dir: Option<tempfile::TempDir>,
}

impl QuickstartTest {
    fn new(config: QuickstartConfig) -> Self {
        let test = Smoketest::builder().autopublish(false).build();
        Self {
            test,
            config,
            project_path: PathBuf::new(),
            _temp_dir: None,
        }
    }

    fn module_name(&self) -> String {
        format!("quickstart-chat-{}", self.config.lang)
    }

    fn doc_path(&self) -> PathBuf {
        workspace_root().join("docs/docs/00100-intro/00300-tutorials/00100-chat-app.md")
    }

    /// Generate the server code from the quickstart documentation.
    fn generate_server(&mut self, server_path: &Path) -> Result<PathBuf> {
        let workspace = workspace_root();
        eprintln!("Generating server code {}: {:?}...", self.config.lang, server_path);

        // Initialize the project (local operation, doesn't need server)
        let output = self.test.spacetime(&[
            "init",
            "--non-interactive",
            "--lang",
            self.config.lang,
            "--project-path",
            server_path.to_str().unwrap(),
            "spacetimedb-project",
        ])?;
        eprintln!("spacetime init output: {}", output);
        println!("[{}] Done spacetime init", Local::now().format("%Y-%m-%d %H:%M:%S"));

        let project_path = server_path.join("spacetimedb");
        self.project_path = project_path.clone();

        // Copy rust-toolchain.toml
        let toolchain_src = workspace.join("rust-toolchain.toml");
        if toolchain_src.exists() {
            fs::copy(&toolchain_src, project_path.join("rust-toolchain.toml"))?;
        }

        // Read and parse the documentation
        let doc_content = fs::read_to_string(self.doc_path())?;
        let server_code = parse_quickstart(&doc_content, self.config.lang, &self.module_name(), true);
        println!("[{}] Done parse_quickstart", Local::now().format("%Y-%m-%d %H:%M:%S"));

        // Write server code
        write_file(&project_path.join(self.config.server_file), &server_code)?;

        // Language-specific server postprocessing
        self.server_postprocess(&project_path)?;

        println!("[{}] Done server_postprocess", Local::now().format("%Y-%m-%d %H:%M:%S"));

        // Build the server (local operation)
        self.test
            .spacetime(&["build", "-d", "-p", project_path.to_str().unwrap()])?;
        println!("[{}] Done spacetime build", Local::now().format("%Y-%m-%d %H:%M:%S"));

        Ok(project_path)
    }

    /// Language-specific server postprocessing.
    fn server_postprocess(&self, server_path: &Path) -> Result<()> {
        let workspace = workspace_root();

        match self.config.lang {
            "rust" => {
                // Write the Cargo.toml with local bindings path
                let bindings_path = workspace.join("crates/bindings");
                let bindings_path_str = bindings_path.display().to_string().replace('\\', "/");

                let cargo_toml = format!(
                    r#"[package]
name = "spacetimedb-quickstart-module"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
spacetimedb = {{ path = "{}", features = ["unstable"] }}
log = "0.4"
"#,
                    bindings_path_str
                );
                fs::write(server_path.join("Cargo.toml"), cargo_toml)?;
            }
            "csharp" => {
                // Set up local NuGet packages
                override_nuget_package(
                    server_path,
                    "SpacetimeDB.Runtime",
                    &workspace.join("crates/bindings-csharp/Runtime"),
                    "bin/Release",
                )?;
                println!(
                    "[{}] Done override_nuget_package SpacetimeDB.Runtime for server_path",
                    Local::now().format("%Y-%m-%d %H:%M:%S")
                );
                override_nuget_package(
                    server_path,
                    "SpacetimeDB.BSATN.Runtime",
                    &workspace.join("crates/bindings-csharp/BSATN.Runtime"),
                    "bin/Release",
                )?;
                println!(
                    "[{}] Done override_nuget_package SpacetimeDB.BSATN.Runtime for server_path",
                    Local::now().format("%Y-%m-%d %H:%M:%S")
                );
            }
            "typescript" => {
                // Build and link the TypeScript SDK
                build_typescript_sdk()?;

                // Uninstall spacetimedb first to avoid pnpm issues
                let _ = pnpm(&["uninstall", "spacetimedb"], server_path);

                // Install the local SDK
                let ts_bindings = workspace.join("crates/bindings-typescript");
                pnpm(&["install", ts_bindings.to_str().unwrap()], server_path)?;
            }
            _ => {}
        }

        Ok(())
    }

    /// Initialize the client project.
    fn project_init(&self, client_path: &Path) -> Result<()> {
        match self.config.client_lang {
            "rust" => {
                let parent = client_path.parent().unwrap();
                run_cmd(
                    &["cargo", "new", "--bin", "--name", "quickstart_chat_client", "client"],
                    parent,
                    None,
                )?;
            }
            "csharp" => {
                run_cmd(
                    &[
                        "dotnet",
                        "new",
                        "console",
                        "--name",
                        "QuickstartChatClient",
                        "--output",
                        client_path.to_str().unwrap(),
                    ],
                    client_path.parent().unwrap(),
                    None,
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Set up the SDK for the client.
    fn sdk_setup(&self, client_path: &Path) -> Result<()> {
        let workspace = workspace_root();

        match self.config.client_lang {
            "rust" => {
                let sdk_rust_path = workspace.join("sdks/rust");
                let sdk_rust_toml_escaped = sdk_rust_path.display().to_string().replace('\\', "\\\\\\\\"); // double escape for toml
                let sdk_rust_toml = format!(
                    "spacetimedb-sdk = {{ path = \"{}\" }}\nlog = \"0.4\"\nhex = \"0.4\"\n",
                    sdk_rust_toml_escaped
                );
                append_to_file(&client_path.join("Cargo.toml"), &sdk_rust_toml)?;
            }
            "csharp" => {
                // Set up NuGet packages for C# SDK
                override_nuget_package(
                    &workspace.join("sdks/csharp"),
                    "SpacetimeDB.BSATN.Runtime",
                    &workspace.join("crates/bindings-csharp/BSATN.Runtime"),
                    "bin/Release",
                )?;
                println!(
                    "[{}] Done override_nuget_package SpacetimeDB.BSATN.Runtime for sdks/csharp",
                    Local::now().format("%Y-%m-%d %H:%M:%S")
                );
                override_nuget_package(
                    &workspace.join("sdks/csharp"),
                    "SpacetimeDB.Runtime",
                    &workspace.join("crates/bindings-csharp/Runtime"),
                    "bin/Release",
                )?;
                println!(
                    "[{}] Done override_nuget_package SpacetimeDB.Runtime for sdks/csharp",
                    Local::now().format("%Y-%m-%d %H:%M:%S")
                );
                override_nuget_package(
                    client_path,
                    "SpacetimeDB.BSATN.Runtime",
                    &workspace.join("crates/bindings-csharp/BSATN.Runtime"),
                    "bin/Release",
                )?;
                println!(
                    "[{}] Done override_nuget_package SpacetimeDB.BSATN.Runtime for client_path",
                    Local::now().format("%Y-%m-%d %H:%M:%S")
                );
                override_nuget_package(
                    client_path,
                    "SpacetimeDB.ClientSDK",
                    &workspace.join("sdks/csharp"),
                    "bin~/Release",
                )?;
                println!(
                    "[{}] Done override_nuget_package SpacetimeDB.ClientSDK for client_path",
                    Local::now().format("%Y-%m-%d %H:%M:%S")
                );

                run_cmd(
                    &["dotnet", "add", "package", "SpacetimeDB.ClientSDK"],
                    client_path,
                    None,
                )?;
                println!(
                    "[{}] Done run_cmd dotnet add package SpacetimeDB.ClientSDK for client_path",
                    Local::now().format("%Y-%m-%d %H:%M:%S")
                );
            }
            _ => {}
        }
        Ok(())
    }

    /// Run the client with input and check output.
    fn check(&self, input: &str, client_path: &Path, contains: &str) -> Result<()> {
        let output = run_cmd(self.config.run_cmd, client_path, Some(input))?;
        eprintln!("Output for {} client:\n{}", self.config.lang, output);

        if !output.contains(contains) {
            bail!("Expected output to contain '{}', but got:\n{}", contains, output);
        }
        Ok(())
    }

    /// Publish the module and return the client path.
    fn publish(&mut self) -> Result<PathBuf> {
        let temp_dir = tempfile::tempdir()?;
        let base_path = temp_dir.path().to_path_buf();
        self._temp_dir = Some(temp_dir);
        let server_path = base_path.join("server");

        self.generate_server(&server_path)?;
        println!("[{}] Done generate_server", Local::now().format("%Y-%m-%d %H:%M:%S"));

        // Publish the module
        let project_path_str = self.project_path.to_str().unwrap().to_string();
        let publish_output = self.test.spacetime(&[
            "publish",
            "--server",
            &self.test.server_url,
            "--project-path",
            &project_path_str,
            "--yes",
            "--clear-database",
            &self.module_name(),
        ])?;
        println!("[{}] Done publish", Local::now().format("%Y-%m-%d %H:%M:%S"));

        // Parse the identity from publish output
        let re = regex::Regex::new(r"identity: ([0-9a-fA-F]+)").unwrap();
        if let Some(caps) = re.captures(&publish_output) {
            let identity = caps.get(1).unwrap().as_str().to_string();
            self.test.database_identity = Some(identity);
        } else {
            bail!(
                "Failed to parse database identity from publish output: {}",
                publish_output
            );
        }

        Ok(base_path.join("client"))
    }

    /// Run the full quickstart test.
    fn run_quickstart(&mut self) -> Result<()> {
        println!("[{}] Start test", Local::now().format("%Y-%m-%d %H:%M:%S"));
        let client_path = self.publish()?;
        println!("[{}] Done full publish", Local::now().format("%Y-%m-%d %H:%M:%S"));

        self.project_init(&client_path)?;
        println!("[{}] Done project_init", Local::now().format("%Y-%m-%d %H:%M:%S"));
        self.sdk_setup(&client_path)?;
        println!("[{}] Done sdk_setup", Local::now().format("%Y-%m-%d %H:%M:%S"));

        panic!("fail test to print output");
    }
}

/// Run the Rust quickstart guides for server and client.
#[test]
fn test_quickstart_rust() {
    let mut qt = QuickstartTest::new(QuickstartConfig::rust());
    qt.run_quickstart().expect("Rust quickstart test failed");
}

/// Run the C# quickstart guides for server and client.
#[test]
fn test_quickstart_csharp() {
    require_dotnet!();

    let mut qt = QuickstartTest::new(QuickstartConfig::csharp());
    qt.run_quickstart().expect("C# quickstart test failed");
}

/// Run the TypeScript quickstart for server (with Rust client).
#[test]
fn test_quickstart_typescript() {
    require_pnpm!();

    let mut qt = QuickstartTest::new(QuickstartConfig::typescript());
    qt.run_quickstart().expect("TypeScript quickstart test failed");
}
