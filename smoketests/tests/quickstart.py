import logging
import re
import tempfile
from pathlib import Path

from .. import Smoketest, MODULES_DIR, STDB_DIR, run_cmd, TEMPLATE_CARGO_TOML, TEST_DIR

DEPENDENCIES = """
log = "0.4"
hex= "0.4"
"""
sdk_path = (STDB_DIR / "crates/sdk").absolute()
escaped_sdk_path = str(sdk_path).replace('\\', '\\\\\\\\')  # double escape for re.sub + toml
DEPENDENCIES_TOML = f'spacetimedb-sdk = {{ path = "{escaped_sdk_path}" }}' + DEPENDENCIES

# The quickstart `main.rs` use a `repl` loop to read user input, so we need to replace it for the smoketest.
TEST = """
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


def parse_quickstart():
    """This method is used to parse the documentation of `docs/docs/sdks/rust/quickstart.md`."""

    doc_path = STDB_DIR / "docs/docs/sdks/rust" / "quickstart.md"
    content = open(doc_path, "r").read()

    # Extract all Rust code blocks from the documentation.
    # This will replicate the steps in the quickstart guide, so if it fails the quickstart guide is broken.
    code_blocks = re.findall(r"```rust\n(.*?)\n```", content, re.DOTALL)

    return "\n".join(code_blocks).replace("user_input_loop(&ctx)", "user_input_direct(&ctx)") + "\n" + TEST


class Quickstart(Smoketest):
    AUTOPUBLISH = False
    MODULE_CODE = ""

    def check(self, input_cmd: str, client_path: Path, contains: str):
        output = run_cmd("cargo", "run", input=input_cmd, cwd=client_path, capture_stderr=True, text=True)
        self.assertIn(contains, output)

    def test_quickstart_rs(self):
        """This test is designed to run the quickstart guide for the Rust SDK."""
        self.project_path = MODULES_DIR / "quickstart-chat"
        self.config_path = self.project_path / "config.toml"
        self.publish_module("quickstart-chat", capture_stderr=True, clear=True)
        client_path = Path(self.enterClassContext(tempfile.TemporaryDirectory()))
        logging.info(f"Generating client code in {client_path}...")
        # Create a cargo project structure
        run_cmd(
            "cargo", "new", "--bin", "quickstart_chat_client",
            cwd=client_path, capture_stderr=True
        )
        client_path = client_path / "quickstart_chat_client"

        open(client_path / "Cargo.toml", "a").write(DEPENDENCIES_TOML)

        # Replay the quickstart guide steps
        main = parse_quickstart()
        open(client_path / "src" / "main.rs", "w").write(main)
        self.spacetime(
            "generate",
            "--lang", "rust",
            "--out-dir", client_path / "src" / "module_bindings",
            "--project-path", self.project_path,
            capture_stderr=True
        )
        logging.info(f"Client code generated in {client_path}.")
        run_cmd(
            "cargo", "build",
            cwd=client_path, capture_stderr=True
        )

        # Replay the quickstart guide steps for test the client
        self.check("", client_path, "connected")
        self.check("/name Alice", client_path, "Alice")
        self.check("Hello World", client_path, "Hello World")
