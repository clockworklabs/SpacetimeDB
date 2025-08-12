import logging
import re
import shutil
from pathlib import Path
import tempfile

import smoketests
from .. import Smoketest, STDB_DIR, run_cmd, TEMPLATE_CARGO_TOML


def _write_file(path: Path, content: str):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content)


def _append_to_file(path: Path, content: str):
    with open(path, "a", encoding="utf-8") as f:
        f.write(content)


def _parse_quickstart(doc_path: Path, language: str) -> str:
    """Extract code blocks from `quickstart.md` docs.
    This will replicate the steps in the quickstart guide, so if it fails the quickstart guide is broken.
    """
    content = Path(doc_path).read_text()
    blocks = re.findall(rf"```{language}\n(.*?)\n```", content, re.DOTALL)

    end = ""
    if language == "csharp":
        found = False
        filtered_blocks = []
        for block in blocks:
            # The doc first create an empy class Module, so we need to fixup the closing
            if "partial class Module" in block:
                block = block.replace("}", "")
                end = "\n}"
            # Remove the first `OnConnected` block, which body is later updated
            if "OnConnected(DbConnection conn" in block and not found:
                found = True
                continue
            filtered_blocks.append(block)
        blocks = filtered_blocks
    # So we could have a different db for each language
    return "\n".join(blocks).replace("quickstart-chat", f"quickstart-chat-{language}") + end


def _dotnet_add_package(project_path: Path, package_name: str, source_path: Path):
    """Add a local NuGet package to a .NET project"""
    sources = run_cmd("dotnet", "nuget", "list", "source", cwd=project_path, capture_stderr=True)
    # Is the source already added?
    if package_name in sources:
        run_cmd("dotnet", "nuget", "remove", "source", package_name, cwd=project_path, capture_stderr=True)
    run_cmd("dotnet", "nuget", "add", "source", source_path, "--name", package_name, cwd=project_path,
            capture_stderr=True)
    run_cmd("dotnet", "add", "package", package_name, cwd=project_path, capture_stderr=True)


class BaseQuickstart(Smoketest):
    AUTOPUBLISH = False
    MODULE_CODE = ""

    lang = None
    server_doc = None
    client_doc = None
    server_file = None
    client_file = None
    module_bindings = None
    extra_code = None
    replacements = {}
    connected_str = None
    run_cmd = []
    build_cmd = []

    def project_init(self, path: Path):
        raise NotImplementedError

    def sdk_setup(self, path: Path):
        raise NotImplementedError

    def _publish(self) -> Path:
        base_path = Path(self.enterClassContext(tempfile.TemporaryDirectory()))

        server_path = base_path / "server"
        self.project_path = server_path
        self.config_path = server_path / "config.toml"

        self.generate_server(server_path)
        self.publish_module(f"quickstart-chat-{self.lang}", capture_stderr=True, clear=True)
        return base_path / "client"

    def generate_server(self, server_path: Path):
        """Generate the server code from the quickstart documentation."""
        logging.info(f"Generating server code {self.lang}: {server_path}...")
        self.spacetime("init", "--lang", self.lang, server_path, capture_stderr=True)
        # Replay the quickstart guide steps
        _write_file(server_path / self.server_file, _parse_quickstart(self.server_doc, self.lang))
        self.server_postprocess(server_path)
        self.spacetime("build", "-d", "-p", server_path, capture_stderr=True)

    def server_postprocess(self, server_path: Path):
        """Optional per-language hook."""
        pass

    def check(self, input_cmd: str, client_path: Path, contains: str):
        """Run the client command and check if the output contains the expected string."""
        output = run_cmd(*self.run_cmd, input=input_cmd, cwd=client_path, capture_stderr=True, text=True)
        print(f"Output for {self.lang} client:\n{output}")
        self.assertIn(contains, output)

    def _test_quickstart(self):
        """Run the quickstart client."""
        client_path = self._publish()
        self.project_init(client_path)
        self.sdk_setup(client_path)

        run_cmd(*self.build_cmd, cwd=client_path, capture_stderr=True)

        self.spacetime(
            "generate", "--lang", self.lang,
            "--out-dir", client_path / self.module_bindings,
            "--project-path", self.project_path, capture_stderr=True
        )
        # Replay the quickstart guide steps
        main = _parse_quickstart(self.client_doc, self.lang)
        for src, dst in self.replacements.items():
            main = main.replace(src, dst)
        main += "\n" + self.extra_code
        _write_file(client_path / self.client_file, main)

        self.check("", client_path, self.connected_str)
        self.check("/name Alice", client_path, "Alice")
        self.check("Hello World", client_path, "Hello World")


class Rust(BaseQuickstart):
    lang = "rust"
    server_doc = STDB_DIR / "docs/docs/modules/rust/quickstart.md"
    client_doc = STDB_DIR / "docs/docs/sdks/rust/quickstart.md"
    server_file = "src/lib.rs"
    client_file = "src/main.rs"
    module_bindings = "src/module_bindings"
    run_cmd = ["cargo", "run"]
    build_cmd = ["cargo", "build"]

    # Replace the interactive user input to allow direct testing
    replacements = {
        "user_input_loop(&ctx)": "user_input_direct(&ctx)"
    }
    extra_code = """
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
"""
    connected_str = "connected"

    def project_init(self, path: Path):
        run_cmd("cargo", "new", "--bin", "--name", "quickstart_chat_client", "client", cwd=path.parent,
                capture_stderr=True)

    def sdk_setup(self, path: Path):
        sdk_rust_path = (STDB_DIR / "crates/sdk").absolute()
        sdk_rust_toml_escaped = str(sdk_rust_path).replace('\\', '\\\\\\\\')  # double escape for re.sub + toml
        sdk_rust_toml = f'spacetimedb-sdk = {{ path = "{sdk_rust_toml_escaped}" }}\nlog = "0.4"\nhex = "0.4"\n'
        _append_to_file(path / "Cargo.toml", sdk_rust_toml)

    def server_postprocess(self, server_path: Path):
        _write_file(server_path / "Cargo.toml", self.cargo_manifest(TEMPLATE_CARGO_TOML))

    def test_quickstart(self):
        """Run the Rust quickstart guides for server and client."""
        self._test_quickstart()


class CSharp(BaseQuickstart):
    lang = "csharp"
    server_doc = STDB_DIR / "docs/docs/modules/c-sharp/quickstart.md"
    client_doc = STDB_DIR / "docs/docs/sdks/c-sharp/quickstart.md"
    server_file = "Lib.cs"
    client_file = "Program.cs"
    module_bindings = "module_bindings"
    run_cmd = ["dotnet", "run"]
    build_cmd = ["dotnet", "build"]

    # Replace the interactive user input to allow direct testing
    replacements = {
        "InputLoop();": "UserInputDirect();",
        ".OnConnect(OnConnected)": ".OnConnect(OnConnectedSignal)",
        ".OnConnectError(OnConnectError)": ".OnConnectError(OnConnectErrorSignal)",
        "Main();": ""  # To put the main function at the end so it can see the new functions
    }
    # So we can wait for the connection to be established...
    extra_code = """
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
"""
    connected_str = "Connected"

    def project_init(self, path: Path):
        run_cmd("dotnet", "new", "console", "--name", "QuickstartChatClient", "--output", path, capture_stderr=True)

    def sdk_setup(self, path: Path):
        _dotnet_add_package(path, "SpacetimeDB.ClientSDK", (STDB_DIR / "sdks/csharp").absolute())

    def server_postprocess(self, server_path: Path):
        _dotnet_add_package(server_path, "SpacetimeDB.Runtime",
                            (STDB_DIR / "crates/bindings-csharp/Runtime").absolute())

    def test_quickstart(self):
        """Run the C# quickstart guides for server and client."""
        if not smoketests.HAVE_DOTNET:
            self.skipTest("C# SDK requires .NET to be installed.")
        self._test_quickstart()
