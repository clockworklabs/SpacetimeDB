import logging
import re
import shutil
from pathlib import Path
import tempfile
import xmltodict

import smoketests
from .. import Smoketest, STDB_DIR, run_cmd, TEMPLATE_CARGO_TOML, TYPESCRIPT_BINDINGS_PATH, build_typescript_sdk, pnpm


def _write_file(path: Path, content: str):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content)


def _append_to_file(path: Path, content: str):
    with open(path, "a", encoding="utf-8") as f:
        f.write(content)


def _parse_quickstart(doc_path: Path, language: str, module_name: str) -> str:
    """Extract code blocks from `quickstart.md` docs.
    This will replicate the steps in the quickstart guide, so if it fails the quickstart guide is broken.
    """
    content = Path(doc_path).read_text()
    codeblock_lang = "ts" if language == "typescript" else language
    blocks = re.findall(rf"```{codeblock_lang}\n(.*?)\n```", content, re.DOTALL)

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
    return "\n".join(blocks).replace("quickstart-chat", module_name) + end

def load_nuget_config(p: Path):
    if p.exists():
        with p.open("rb") as f:
            return xmltodict.parse(f.read(), force_list=["add", "packageSource", "package"])
    return {}

def save_nuget_config(p: Path, doc: dict):
    # Write back (pretty, UTF-8, no BOM)
    xml = xmltodict.unparse(doc, pretty=True)
    p.write_text(xml, encoding="utf-8")

def add_source(doc: dict, *, key: str, path: str) -> None:
    cfg = doc.setdefault("configuration", {})
    sources = cfg.setdefault("packageSources", {})
    source_entries = sources.setdefault("add", [])
    source = {"@key": key, "@value": path}
    source_entries.append(source)

def add_mapping(doc: dict, *, key: str, pattern: str) -> None:
    cfg = doc.setdefault("configuration", {})

    psm = cfg.setdefault("packageSourceMapping", {})
    mapping_sources = psm.setdefault("packageSource", [])

    # Find or create the target <packageSource key="...">
    target = next((s for s in mapping_sources if s.get("@key") == key), None)
    if target is None:
        target = {"@key": key, "package": []}
        mapping_sources.append(target)

    pkgs = target.setdefault("package", [])

    existing = {pkg.get("@pattern") for pkg in pkgs if "@pattern" in pkg}
    if pattern not in existing:
        pkgs.append({"@pattern": pattern})

def override_nuget_package(*, project_dir: Path, package: str, source_dir: Path, build_subdir: str):
    """Override nuget config to use a local NuGet package on a .NET project"""
    # Make sure the local package is built
    run_cmd("dotnet", "pack", cwd=source_dir)

    p = Path(project_dir) / "nuget.config"
    doc = load_nuget_config(p)
    add_source(doc, key=package, path=source_dir/build_subdir)
    add_mapping(doc, key=package, pattern=package)
    # Fallback for other packages
    add_mapping(doc, key="nuget.org", pattern="*")
    save_nuget_config(p, doc)

    # Clear any caches for nuget packages
    run_cmd("dotnet", "nuget", "locals", "--clear", "all", capture_stderr=True)

class BaseQuickstart(Smoketest):
    AUTOPUBLISH = False
    MODULE_CODE = ""

    lang = None
    client_lang = None
    codeblock_langs = None
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

    @property
    def _module_name(self):
        return f"quickstart-chat-{self.lang}"

    def _publish(self) -> Path:
        base_path = Path(self.enterClassContext(tempfile.TemporaryDirectory()))
        server_path = base_path / "server"

        self.generate_server(server_path)
        self.publish_module(self._module_name, capture_stderr=True, clear=True)
        return base_path / "client"

    def generate_server(self, server_path: Path):
        """Generate the server code from the quickstart documentation."""
        logging.info(f"Generating server code {self.lang}: {server_path}...")
        self.spacetime(
            "init",
            "--non-interactive",
            "--lang",
            self.lang,
            "--project-path",
            server_path,
            "spacetimedb-project",
            capture_stderr=True,
        )
        self.project_path = server_path / "spacetimedb"
        shutil.copy2(STDB_DIR / "rust-toolchain.toml", self.project_path)
        _write_file(self.project_path / self.server_file, _parse_quickstart(self.server_doc, self.lang, self._module_name))
        self.server_postprocess(self.project_path)
        self.spacetime("build", "-d", "-p", self.project_path, capture_stderr=True)

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

        client_lang = self.client_lang or self.lang
        self.spacetime(
            "generate", "--lang", client_lang,
            "--out-dir", client_path / self.module_bindings,
            "--project-path", self.project_path, capture_stderr=True
        )
        # Replay the quickstart guide steps
        main = _parse_quickstart(self.client_doc, client_lang, self._module_name)
        for src, dst in self.replacements.items():
            main = main.replace(src, dst)
        main += "\n" + self.extra_code
        server = self.get_server_address()
        host = server["address"]
        protocol = server["protocol"]
        main = main.replace("http://localhost:3000", f"{protocol}://{host}")
        _write_file(client_path / self.client_file, main)

        self.check("", client_path, self.connected_str)
        self.check("/name Alice", client_path, "Alice")
        self.check("Hello World", client_path, "Hello World")


class Rust(BaseQuickstart):
    lang = "rust"
    server_doc = STDB_DIR / "docs/docs/06-Server Module Languages/02-rust-quickstart.md"
    client_doc = STDB_DIR / "docs/docs/07-Client SDK Languages/04-rust-quickstart.md"
    server_file = "src/lib.rs"
    client_file = "src/main.rs"
    module_bindings = "src/module_bindings"
    run_cmd = ["cargo", "run"]
    build_cmd = ["cargo", "build"]

    replacements = {
        # Replace the interactive user input to allow direct testing
        "user_input_loop(&ctx)": "user_input_direct(&ctx)",
        # Don't cache the token, because it will cause the test to fail if we run against a non-default server (because  we don't cache the corresponding signing keypair)
        ".with_token(creds_store()": "//.with_token(creds_store()"
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
        sdk_rust_path = (STDB_DIR / "sdks/rust").absolute()
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
    server_doc = STDB_DIR / "docs/docs/06-Server Module Languages/04-csharp-quickstart.md"
    client_doc = STDB_DIR / "docs/docs/07-Client SDK Languages/02-csharp-quickstart.md"
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
        # Don't cache the token, because it will cause the test to fail if we run against a non-default server (because  we don't cache the corresponding signing keypair)
        ".WithToken(AuthToken.Token)": "//.WithToken(AuthToken.Token)",
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
        override_nuget_package(
            project_dir=STDB_DIR/"sdks/csharp",
            package="SpacetimeDB.BSATN.Runtime",
            source_dir=(STDB_DIR / "crates/bindings-csharp/BSATN.Runtime").absolute(),
            build_subdir="bin/Release"
        )
        # This one is only needed because the regression-tests subdir uses it
        override_nuget_package(
            project_dir=STDB_DIR/"sdks/csharp",
            package="SpacetimeDB.Runtime",
            source_dir=(STDB_DIR / "crates/bindings-csharp/Runtime").absolute(),
            build_subdir="bin/Release"
        )
        override_nuget_package(
            project_dir=path,
            package="SpacetimeDB.BSATN.Runtime",
            source_dir=(STDB_DIR / "crates/bindings-csharp/BSATN.Runtime").absolute(),
            build_subdir="bin/Release"
        )
        override_nuget_package(
            project_dir=path,
            package="SpacetimeDB.ClientSDK",
            source_dir=(STDB_DIR / "sdks/csharp").absolute(),
            build_subdir="bin~/Release"
        )
        run_cmd("dotnet", "add", "package", "SpacetimeDB.ClientSDK", cwd=path, capture_stderr=True)

    def server_postprocess(self, server_path: Path):
        override_nuget_package(
            project_dir=server_path,
            package="SpacetimeDB.Runtime",
            source_dir=(STDB_DIR / "crates/bindings-csharp/Runtime").absolute(),
            build_subdir="bin/Release"
        )
        override_nuget_package(
            project_dir=server_path,
            package="SpacetimeDB.BSATN.Runtime",
            source_dir=(STDB_DIR / "crates/bindings-csharp/BSATN.Runtime").absolute(),
            build_subdir="bin/Release"
        )

    def test_quickstart(self):
        """Run the C# quickstart guides for server and client."""
        if not smoketests.HAVE_DOTNET:
            self.skipTest("C# SDK requires .NET to be installed.")
        self._test_quickstart()

# We use the Rust client for testing the TypeScript server quickstart because
# the TypeScript client quickstart is a React app, which is difficult to
# smoketest.
class TypeScript(Rust):
    lang = "typescript"
    client_lang = "rust"
    server_doc = STDB_DIR / "docs/docs/06-Server Module Languages/05-typescript-quickstart.md"
    server_file = "src/index.ts"

    def server_postprocess(self, server_path: Path):
        build_typescript_sdk()
        pnpm("install", TYPESCRIPT_BINDINGS_PATH, cwd=server_path)

    def test_quickstart(self):
        """Run the TypeScript quickstart guides for server."""
        self._test_quickstart()
