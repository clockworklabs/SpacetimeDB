from pathlib import Path
import unittest
import tempfile
import os
import re
import string
import random
import string
import subprocess
import json
import sys

TEST_DIR = Path(__file__).parent
STDB_DIR = TEST_DIR.parent
SPACETIME_BIN = STDB_DIR / "target/debug/spacetime"
STDB_CONFIG = TEST_DIR / "config.toml"

TEMPLATE_LIB_RS = open(STDB_DIR / "crates/cli/src/subcommands/project/rust/lib._rs").read()
TEMPLATE_CARGO_TOML = open(STDB_DIR / "crates/cli/src/subcommands/project/rust/Cargo._toml").read()
bindings_path = (STDB_DIR / "crates/bindings").absolute()
TEMPLATE_CARGO_TOML = (re.compile(r"^spacetimedb\s*=.*$", re.M) \
    .sub(f'spacetimedb = {{ path = "{bindings_path}" }}', TEMPLATE_CARGO_TOML))


def random_string(k=20):
    return ''.join(random.choices(string.ascii_letters, k=k))

def extract_fields(cmd_output, field_name):
    out = []
    for line in cmd_output.splitlines():
        fields = line.split()
        if len(fields) < 2:
            continue
        label, val, *_ = fields
        if label == field_name:
            out.append(val)
    return out

def extract_field(cmd_output, field_name):
    field, = extract_fields(cmd_output, field_name)
    return field

def run_cmd(*args, capture_stderr=True, check=True, full_output=False, cmd_name=None, **kwargs):
    output = subprocess.run(
        list(args),
        encoding="utf8",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE if capture_stderr else None,
        **kwargs
    )
    if capture_stderr:
        sys.stderr.write(output.stderr)
        sys.stderr.flush()
    if check:
        if cmd_name is not None:
            output.args[0] = "spacetime"
        output.check_returncode()
    return output if full_output else output.stdout
    

def spacetime(*args, **kwargs):
    return run_cmd(SPACETIME_BIN, *args, cmd_name="spacetime", **kwargs)

class Smoketest(unittest.TestCase):
    MODULE_CODE = TEMPLATE_LIB_RS
    AUTOPUBLISH = True
    EXTRA_DEPS = ""

    @classmethod
    def cargo_manifest(cls, manifest_text):
        return manifest_text + cls.EXTRA_DEPS

    # helpers

    spacetime = staticmethod(spacetime)

    def _check_published(self):
        if not hasattr(self, "address"):
            raise Exception("Cannot use this function without publishing a module")
    
    def call(self, reducer, *args):
        self._check_published()
        self.spacetime("call", "--", self.address, reducer, *map(json.dumps, args))
    
    def logs(self, n):
        return [log["message"] for log in self.log_records(n)]
        
    def log_records(self, n):
        self._check_published()
        logs = self.spacetime("logs", "--json", "--", self.address, str(n))
        return list(map(json.loads, logs.splitlines()))

    def publish_module(self, domain=None, *, clear=True, capture_stderr=True):
        publish_output = self.spacetime(
            "publish", "-S",
            *[domain] if domain is not None else [],
            "--project-path", self.project_path,
            *(["-c"] if clear else []),
            capture_stderr=capture_stderr,
        )
        self.address = domain if domain is not None else re.search(r"address: ([0-9a-fA-F]+)", publish_output)[1]
    
    @classmethod
    def reset_config(cls):
        open(cls.project_path / "config.toml", "w").write(open(STDB_CONFIG).read())

    def fingerprint(self):
        # Fetch the server's fingerprint; required for `identity list`.
        self.spacetime("server", "fingerprint", "localhost", "-f")
    
    def new_identity(self, *, email, default=False):
        output = self.spacetime("identity", "new", "--no-email" if email is None else f"--email={email}")
        identity = extract_field(output, "IDENTITY")
        if default:
            self.spacetime("identity", "set-default", identity)
        return identity
    
    def token(self, identity):
        return self.spacetime("identity", "token", identity).strip()
    
    def import_identity(self, identity, token, *, default=False):
        self.spacetime("identity", "import", identity, token)
        if default:
            self.spacetime("identity", "set-default", identity)

    @classmethod
    def write_module_code(cls, module_code):
        open(cls.project_path / "src/lib.rs", "w").write(module_code)

    # testcase initialization
    
    
    @classmethod
    def setUpClass(cls):
        cls._project_dir = tempfile.TemporaryDirectory()
        cls.project_path = Path(cls._project_dir.name)
        cls.reset_config()
        os.environ["SPACETIME_CONFIG_FILE"] = str(cls.project_path / "config.toml")
        open(cls.project_path / "Cargo.toml", "w").write(cls.cargo_manifest(TEMPLATE_CARGO_TOML))
        os.mkdir(cls.project_path / "src")
        cls.write_module_code(cls.MODULE_CODE)

        if cls.AUTOPUBLISH:
            print(f"Compiling module for {cls.__qualname__}...", file=sys.__stderr__)
            cls.publish_module(cls, capture_stderr=False)
    
    def tearDown(self):
        # if this single test method published a database, clean it up now
        if "address" in self.__dict__:
            try:
                # TODO: save the credentials in publish_module()
                self.spacetime("delete", self.address, capture_stderr=False)
            except Exception:
                pass
    
    @classmethod
    def tearDownClass(cls):
        try:
            if hasattr(cls, "address"):
                try:
                    # TODO: save the credentials in publish_module()
                    cls.spacetime("delete", cls.address, capture_stderr=False)
                except Exception:
                    pass
                cls._project_dir.cleanup()
        finally:
            os.environ["SPACETIME_CONFIG_FILE"] = ""

    # def setUp(self):
    #     if self.AUTOPUBLISH:
    #         self.spacetime("publish", "-S", "--project-path", self.project_path)
    